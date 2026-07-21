use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use profiler_core::{
    ErrorCode, FormatIdentifierIdentity, FormatMatch, FormatObservation, FormatState,
    FormatToolIdentity, FormatWorkItem, ProfilerError, ProfilerResult,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;
use tracing::{debug, warn};

pub const RECOMMENDED_SIEGFRIED_VERSION: &str = "1.11.6";
pub const RECOMMENDED_PRONOM_VERSION: &str = "v124";

#[derive(Debug, Clone)]
pub struct SiegfriedOptions {
    pub executable: Option<PathBuf>,
    pub signature: Option<PathBuf>,
    pub workers: u32,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct SiegfriedRunner {
    executable: PathBuf,
    signature: Option<PathBuf>,
    workers: u32,
    timeout: Duration,
    identity: FormatToolIdentity,
}

#[derive(Debug, Clone)]
pub struct ResolvedFormatInput {
    pub item: FormatWorkItem,
    pub source_path: PathBuf,
}

#[derive(Debug)]
pub struct BatchWorkspace {
    directory: TempDir,
}

impl BatchWorkspace {
    pub fn new(parent: &Path, format_run_id: &str, sequence: u64) -> ProfilerResult<Self> {
        let staging_root = parent
            .join("format-staging")
            .join(format_run_id)
            .join(format!("batch-{sequence:08}"));
        fs::create_dir_all(&staging_root).map_err(|source| ProfilerError::Io {
            operation: "creating exact-format staging directory",
            path: staging_root.clone(),
            source,
        })?;
        let directory = tempfile::Builder::new()
            .prefix("work-")
            .tempdir_in(&staging_root)
            .map_err(|source| ProfilerError::Io {
                operation: "creating exact-format batch workspace",
                path: staging_root,
                source,
            })?;
        Ok(Self { directory })
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }
}

impl SiegfriedRunner {
    pub fn probe(options: &SiegfriedOptions) -> ProfilerResult<Self> {
        let executable = discover_executable(options.executable.as_deref())?;
        let signature = options
            .signature
            .clone()
            .or_else(|| discover_signature(&executable))
            .ok_or_else(|| {
                ProfilerError::contract(
                    ErrorCode::FormatToolIncompatible,
                    "the pinned Siegfried default.sig signature file was not found",
                    false,
                )
            })?;
        let probe_directory = tempfile::tempdir().map_err(|source| ProfilerError::Io {
            operation: "creating Siegfried probe directory",
            path: std::env::temp_dir(),
            source,
        })?;
        let probe_file = probe_directory.path().join("probe.bin");
        File::create(&probe_file).map_err(|source| ProfilerError::Io {
            operation: "creating Siegfried probe file",
            path: probe_file.clone(),
            source,
        })?;
        let signature_path = fs::canonicalize(&signature).map_err(|source| ProfilerError::Io {
            operation: "canonicalizing Siegfried signature file",
            path: signature.clone(),
            source,
        })?;
        let output = run_command(
            &executable,
            Some(signature_path.as_path()),
            options.workers.max(1),
            Duration::from_secs(30),
            &[probe_file],
            probe_directory.path(),
        )?;
        let parsed = parse_output(&output.stdout)?;
        let executable_sha256 = sha256_file(&executable)?;
        let signature_sha256 = Some(sha256_file(&signature_path)?);
        let signature_version =
            extract_pronom_version(&parsed.identifiers).unwrap_or_else(|| "unknown".into());
        if parsed.siegfried.trim() != RECOMMENDED_SIEGFRIED_VERSION {
            return Err(ProfilerError::contract_with_context(
                ErrorCode::FormatToolIncompatible,
                "Siegfried version does not match the profiler release contract",
                false,
                BTreeMap::from([
                    (
                        "requiredVersion".into(),
                        RECOMMENDED_SIEGFRIED_VERSION.into(),
                    ),
                    ("observedVersion".into(), parsed.siegfried.clone()),
                ]),
            ));
        }
        if signature_version != RECOMMENDED_PRONOM_VERSION {
            return Err(ProfilerError::contract_with_context(
                ErrorCode::FormatToolIncompatible,
                "PRONOM signature version does not match the profiler release contract",
                false,
                BTreeMap::from([
                    (
                        "requiredSignature".into(),
                        RECOMMENDED_PRONOM_VERSION.into(),
                    ),
                    ("observedSignature".into(), signature_version.clone()),
                ]),
            ));
        }
        let identity = FormatToolIdentity {
            tool_name: "siegfried".into(),
            tool_version: parsed.siegfried,
            executable_path: executable.to_string_lossy().into_owned(),
            executable_sha256,
            signature_path: signature_path.to_string_lossy().into_owned(),
            signature_sha256,
            signature_version,
            signature_created: parsed.created,
            identifiers: parsed
                .identifiers
                .into_iter()
                .map(|identifier| FormatIdentifierIdentity {
                    name: identifier.name,
                    details: identifier.details,
                })
                .collect(),
            probed_at: now_text(),
        };
        Ok(Self {
            executable,
            signature: Some(signature_path),
            workers: options.workers.max(1),
            timeout: options.timeout,
            identity,
        })
    }

    pub fn identity(&self) -> &FormatToolIdentity {
        &self.identity
    }

    pub fn identify_batch(
        &self,
        inputs: &[ResolvedFormatInput],
        staging: &BatchWorkspace,
    ) -> ProfilerResult<Vec<FormatObservation>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        self.identify_batch_adaptive(inputs, staging.path(), 0)
    }

    fn identify_batch_adaptive(
        &self,
        inputs: &[ResolvedFormatInput],
        staging_root: &Path,
        depth: u32,
    ) -> ProfilerResult<Vec<FormatObservation>> {
        match self.identify_batch_once(inputs, staging_root, depth) {
            Ok(observations) => Ok(observations),
            Err(error) if inputs.len() > 1 => {
                warn!(count = inputs.len(), depth, error = %error, "splitting failed Siegfried batch");
                let middle = inputs.len() / 2;
                let mut left =
                    self.identify_batch_adaptive(&inputs[..middle], staging_root, depth + 1)?;
                left.extend(self.identify_batch_adaptive(
                    &inputs[middle..],
                    staging_root,
                    depth + 1,
                )?);
                Ok(left)
            }
            Err(error) => Ok(vec![tool_error_observation(&inputs[0], &error)]),
        }
    }

    fn identify_batch_once(
        &self,
        inputs: &[ResolvedFormatInput],
        staging_root: &Path,
        depth: u32,
    ) -> ProfilerResult<Vec<FormatObservation>> {
        let subdirectory = staging_root.join(format!("attempt-{depth}-{}", inputs.len()));
        fs::create_dir_all(&subdirectory).map_err(|source| ProfilerError::Io {
            operation: "creating Siegfried attempt directory",
            path: subdirectory.clone(),
            source,
        })?;
        let mut paths = Vec::with_capacity(inputs.len());
        let mut by_key = HashMap::with_capacity(inputs.len());
        let mut staging_modes = HashMap::with_capacity(inputs.len());
        for input in inputs {
            let alias = safe_alias_path(&subdirectory, &input.item);
            let (scan_path, staging_mode) = match create_symlink_alias(&input.source_path, &alias) {
                Ok(()) => (alias, "symlink_alias".to_owned()),
                Err(error) => {
                    debug!(path = %input.source_path.display(), error = %error, "symbolic-link alias unavailable; using canonical object path without extension evidence");
                    (input.source_path.clone(), "canonical_path".to_owned())
                }
            };
            let key = path_key(&scan_path);
            by_key.insert(key.clone(), input);
            staging_modes.insert(key, staging_mode);
            paths.push(scan_path);
        }
        let output = run_command(
            &self.executable,
            self.signature.as_deref(),
            self.workers,
            self.timeout,
            &paths,
            &subdirectory,
        )?;
        let parsed = parse_output(&output.stdout)?;
        let mut observations = Vec::with_capacity(inputs.len());
        let mut seen = HashMap::<String, usize>::new();
        for file in parsed.files {
            let key = path_key(Path::new(&file.filename));
            let Some(input) = by_key.get(&key) else {
                continue;
            };
            *seen.entry(input.item.sha256.clone()).or_default() += 1;
            observations.push(observation_from_file(
                input,
                &file,
                staging_modes
                    .get(&key)
                    .map_or("canonical_path", String::as_str),
            ));
        }
        for input in inputs {
            if !seen.contains_key(&input.item.sha256) {
                observations.push(FormatObservation {
                    content_object_id: input.item.content_object_id.clone(),
                    sha256: input.item.sha256.clone(),
                    state: FormatState::ToolError,
                    source_mime_type: input.item.source_mime_type.clone(),
                    preferred_extension: input.item.preferred_extension.clone(),
                    staging_mode: "unmatched_output".into(),
                    primary_identifier: None,
                    primary_format_name: None,
                    primary_format_version: None,
                    primary_mime_type: None,
                    match_count: 0,
                    extension_checked: false,
                    extension_mismatch: false,
                    error_code: Some("OUTPUT_RECORD_MISSING".into()),
                    error_message: Some("Siegfried returned no result for the input object".into()),
                    matches: Vec::new(),
                    observed_at: now_text(),
                });
            }
        }
        observations.sort_by(|left, right| left.sha256.cmp(&right.sha256));
        Ok(observations)
    }
}

#[derive(Debug)]
struct ProcessOutput {
    stdout: Vec<u8>,
}

fn append_signature_arguments(command: &mut Command, signature: &Path) -> ProfilerResult<()> {
    let signature_home = signature
        .parent()
        .ok_or_else(|| ProfilerError::InvalidPath {
            message: "Siegfried signature path has no parent directory".into(),
            path: signature.to_path_buf(),
        })?;
    let signature_name = signature
        .file_name()
        .ok_or_else(|| ProfilerError::InvalidPath {
            message: "Siegfried signature path has no filename".into(),
            path: signature.to_path_buf(),
        })?;

    // Siegfried 1.11.6's JSON writer escapes scanned filenames but interpolates
    // the header's signature value verbatim. An absolute Windows path therefore
    // emits invalid JSON because backslashes are not escaped. Resolve the
    // signature through -home and pass only the filename; this also keeps
    // evidence output independent of private installation paths.
    command
        .arg("-home")
        .arg(signature_home)
        .arg("-sig")
        .arg(signature_name);
    Ok(())
}

fn run_command(
    executable: &Path,
    signature: Option<&Path>,
    workers: u32,
    timeout: Duration,
    paths: &[PathBuf],
    working_directory: &Path,
) -> ProfilerResult<ProcessOutput> {
    let list_path = write_input_list(paths, working_directory)?;
    let command = build_siegfried_command(
        executable,
        signature,
        workers,
        &list_path,
        working_directory,
    )?;
    execute_siegfried(command, executable, timeout)
}

fn write_input_list(paths: &[PathBuf], working_directory: &Path) -> ProfilerResult<PathBuf> {
    let list_path = working_directory.join("inputs.txt");
    let mut list_file = File::create(&list_path).map_err(|source| ProfilerError::Io {
        operation: "creating Siegfried input list",
        path: list_path.clone(),
        source,
    })?;
    for path in paths {
        let display = path.to_string_lossy();
        if display.contains('\r') || display.contains('\n') {
            return Err(ProfilerError::InvalidPath {
                message: "Siegfried input path contains a line break".into(),
                path: path.clone(),
            });
        }
        writeln!(list_file, "{display}").map_err(|source| ProfilerError::Io {
            operation: "writing Siegfried input list",
            path: list_path.clone(),
            source,
        })?;
    }
    list_file.sync_all().map_err(|source| ProfilerError::Io {
        operation: "syncing Siegfried input list",
        path: list_path.clone(),
        source,
    })?;
    Ok(list_path)
}

fn build_siegfried_command(
    executable: &Path,
    signature: Option<&Path>,
    workers: u32,
    list_path: &Path,
    working_directory: &Path,
) -> ProfilerResult<Command> {
    let mut command = Command::new(executable);
    command
        .arg("-json")
        .arg("-utc")
        .arg("-coe")
        .arg("-sym")
        .arg("-multi")
        .arg(workers.to_string());
    if let Some(signature) = signature {
        append_signature_arguments(&mut command, signature)?;
    }
    command
        .arg("-f")
        .arg(list_path)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

fn execute_siegfried(
    mut command: Command,
    executable: &Path,
    timeout: Duration,
) -> ProfilerResult<ProcessOutput> {
    let mut child = command.spawn().map_err(|source| ProfilerError::Io {
        operation: "starting Siegfried sidecar",
        path: executable.to_path_buf(),
        source,
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProfilerError::Internal("Siegfried stdout pipe was not available".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ProfilerError::Internal("Siegfried stderr pipe was not available".into()))?;
    let stdout_thread = spawn_limited_reader(stdout, 64 * 1024 * 1024);
    let stderr_thread = spawn_limited_reader(stderr, 4 * 1024 * 1024);
    let status = match wait_for_process(&mut child, executable, timeout) {
        Ok(status) => status,
        Err(error) => {
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(error);
        }
    };
    let stdout_result = join_reader(stdout_thread, executable, "stdout");
    let stderr_result = join_reader(stderr_thread, executable, "stderr");
    let stdout = stdout_result?;
    let stderr = stderr_result?;
    if !status.success() {
        return Err(ProfilerError::contract_with_context(
            ErrorCode::FormatRunFailed,
            "Siegfried sidecar returned a non-zero exit status",
            true,
            BTreeMap::from([
                ("exitStatus".into(), status.to_string()),
                (
                    "stderr".into(),
                    truncate_text(&String::from_utf8_lossy(&stderr), 4_096),
                ),
            ]),
        ));
    }
    Ok(ProcessOutput { stdout })
}

fn spawn_limited_reader<R>(reader: R, limit: usize) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = BufReader::new(reader);
        read_to_end_limited(&mut reader, &mut bytes, limit).map(|()| bytes)
    })
}

fn wait_for_process(
    child: &mut std::process::Child,
    executable: &Path,
    timeout: Duration,
) -> ProfilerResult<std::process::ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|source| ProfilerError::Io {
            operation: "polling Siegfried sidecar",
            path: executable.to_path_buf(),
            source,
        })? {
            return Ok(status);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProfilerError::contract_with_context(
                ErrorCode::FormatRunFailed,
                "Siegfried batch exceeded its configured timeout",
                true,
                BTreeMap::from([("timeoutSeconds".into(), timeout.as_secs().to_string())]),
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn join_reader(
    handle: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    executable: &Path,
    stream: &'static str,
) -> ProfilerResult<Vec<u8>> {
    handle
        .join()
        .map_err(|_| ProfilerError::Internal(format!("Siegfried {stream} reader panicked")))?
        .map_err(|source| ProfilerError::Io {
            operation: if stream == "stdout" {
                "reading Siegfried stdout"
            } else {
                "reading Siegfried stderr"
            },
            path: executable.to_path_buf(),
            source,
        })
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    #[serde(default)]
    siegfried: String,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    identifiers: Vec<RawIdentifier>,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(Debug, Deserialize)]
struct RawIdentifier {
    #[serde(default)]
    name: String,
    #[serde(default)]
    details: String,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    #[serde(default)]
    filename: String,
    #[serde(default)]
    errors: serde_json::Value,
    #[serde(default)]
    matches: Vec<RawMatch>,
}

#[derive(Debug, Deserialize)]
struct RawMatch {
    #[serde(default, rename = "ns")]
    namespace: String,
    #[serde(default, rename = "id")]
    identifier: String,
    #[serde(default, rename = "format")]
    format_name: String,
    #[serde(default, rename = "version")]
    format_version: String,
    #[serde(default, rename = "mime")]
    mime_type: String,
    #[serde(default, rename = "class")]
    format_class: Option<String>,
    #[serde(default)]
    basis: String,
    #[serde(default)]
    warning: String,
}

fn parse_output(bytes: &[u8]) -> ProfilerResult<RawOutput> {
    serde_json::from_slice(bytes).map_err(|error| {
        ProfilerError::contract_with_context(
            ErrorCode::FormatOutputInvalid,
            "Siegfried returned invalid JSON output",
            false,
            BTreeMap::from([
                ("parserError".into(), error.to_string()),
                (
                    "outputPrefix".into(),
                    truncate_text(&String::from_utf8_lossy(bytes), 2_048),
                ),
            ]),
        )
    })
}

fn observation_from_file(
    input: &ResolvedFormatInput,
    file: &RawFile,
    staging_mode: &str,
) -> FormatObservation {
    let error_message = normalize_errors(&file.errors);
    let mut matches = file
        .matches
        .iter()
        .map(|raw| FormatMatch {
            namespace: raw.namespace.clone(),
            identifier: raw.identifier.clone(),
            format_name: raw.format_name.clone(),
            format_version: raw.format_version.clone(),
            mime_type: raw.mime_type.clone(),
            format_class: raw.format_class.clone().filter(|value| !value.is_empty()),
            basis: raw.basis.clone(),
            warning: raw.warning.clone(),
            is_primary: false,
        })
        .collect::<Vec<_>>();
    let primary_index = choose_primary_match(&matches);
    if let Some(index) = primary_index
        && let Some(primary) = matches.get_mut(index)
    {
        primary.is_primary = true;
    }
    let has_signature_evidence = matches
        .iter()
        .any(|format_match| is_viable_match(format_match) && !is_extension_only(format_match));
    let viable_identifiers = matches
        .iter()
        .filter(|format_match| {
            is_viable_match(format_match)
                && (!has_signature_evidence || !is_extension_only(format_match))
        })
        .map(|format_match| (&format_match.namespace, &format_match.identifier))
        .collect::<std::collections::BTreeSet<_>>();
    let state = if error_message.is_some() && matches.is_empty() {
        FormatState::ToolError
    } else if viable_identifiers.is_empty() {
        FormatState::Unknown
    } else if viable_identifiers.len() > 1 {
        FormatState::Ambiguous
    } else {
        FormatState::Identified
    };
    let primary = primary_index.and_then(|index| matches.get(index));
    let extension_checked =
        input.item.preferred_extension.is_some() && staging_mode == "symlink_alias";
    let extension_mismatch = extension_checked
        && matches.iter().any(|format_match| {
            let warning = format_match.warning.to_ascii_lowercase();
            warning.contains("extension mismatch")
                || warning.contains("filename mismatch")
                || warning.contains("extension does not match")
        });
    FormatObservation {
        content_object_id: input.item.content_object_id.clone(),
        sha256: input.item.sha256.clone(),
        state,
        source_mime_type: input.item.source_mime_type.clone(),
        preferred_extension: input.item.preferred_extension.clone(),
        staging_mode: staging_mode.into(),
        primary_identifier: primary.map(|value| value.identifier.clone()),
        primary_format_name: primary.map(|value| value.format_name.clone()),
        primary_format_version: primary.map(|value| value.format_version.clone()),
        primary_mime_type: primary.map(|value| value.mime_type.clone()),
        match_count: u64::try_from(matches.len()).unwrap_or(u64::MAX),
        extension_checked,
        extension_mismatch,
        error_code: error_message
            .as_ref()
            .map(|_| "SIEGFRIED_FILE_ERROR".into()),
        error_message,
        matches,
        observed_at: now_text(),
    }
}

fn choose_primary_match(matches: &[FormatMatch]) -> Option<usize> {
    matches
        .iter()
        .enumerate()
        .filter(|(_, format_match)| is_viable_match(format_match))
        .max_by_key(|(index, format_match)| (match_score(format_match), std::cmp::Reverse(*index)))
        .map(|(index, _)| index)
}

fn is_viable_match(format_match: &FormatMatch) -> bool {
    !format_match.identifier.trim().is_empty()
        && !format_match.identifier.eq_ignore_ascii_case("UNKNOWN")
}

fn is_extension_only(format_match: &FormatMatch) -> bool {
    let basis = format_match.basis.to_ascii_lowercase();
    basis.contains("extension match")
        && !basis.contains("byte match")
        && !basis.contains("container match")
        && !basis.contains("xml match")
        && !basis.contains("text match")
}

fn match_score(format_match: &FormatMatch) -> u32 {
    let basis = format_match.basis.to_ascii_lowercase();
    let mut score = if basis.contains("container match") {
        500
    } else if basis.contains("byte match") {
        400
    } else if basis.contains("xml match") {
        350
    } else if basis.contains("text match") {
        300
    } else if basis.contains("extension match") {
        100
    } else {
        200
    };
    if format_match.namespace.eq_ignore_ascii_case("pronom") {
        score += 30;
    }
    if format_match.warning.trim().is_empty() {
        score += 20;
    }
    score
}

fn tool_error_observation(input: &ResolvedFormatInput, error: &ProfilerError) -> FormatObservation {
    FormatObservation {
        content_object_id: input.item.content_object_id.clone(),
        sha256: input.item.sha256.clone(),
        state: FormatState::ToolError,
        source_mime_type: input.item.source_mime_type.clone(),
        preferred_extension: input.item.preferred_extension.clone(),
        staging_mode: "batch_isolation".into(),
        primary_identifier: None,
        primary_format_name: None,
        primary_format_version: None,
        primary_mime_type: None,
        match_count: 0,
        extension_checked: false,
        extension_mismatch: false,
        error_code: Some(format!("{:?}", error.report().code).to_ascii_uppercase()),
        error_message: Some(error.to_string()),
        matches: Vec::new(),
        observed_at: now_text(),
    }
}

pub fn skipped_observation(item: &FormatWorkItem) -> FormatObservation {
    let state = if item.expected_size_bytes == 0 {
        FormatState::Empty
    } else {
        FormatState::SkippedUnavailable
    };
    FormatObservation {
        content_object_id: item.content_object_id.clone(),
        sha256: item.sha256.clone(),
        state,
        source_mime_type: item.source_mime_type.clone(),
        preferred_extension: item.preferred_extension.clone(),
        staging_mode: "not_invoked".into(),
        primary_identifier: None,
        primary_format_name: None,
        primary_format_version: None,
        primary_mime_type: None,
        match_count: 0,
        extension_checked: false,
        extension_mismatch: false,
        error_code: None,
        error_message: None,
        matches: Vec::new(),
        observed_at: now_text(),
    }
}

fn create_symlink_alias(source: &Path, alias: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(source, alias)
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, alias)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (source, alias);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "symbolic file aliases are unsupported on this platform",
        ))
    }
}

fn read_to_end_limited<R: Read>(
    reader: &mut R,
    target: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<()> {
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        if target.len().saturating_add(read) > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Siegfried output exceeded the configured safety limit",
            ));
        }
        target.extend_from_slice(&buffer[..read]);
    }
}

fn safe_alias_path(directory: &Path, item: &FormatWorkItem) -> PathBuf {
    match item.preferred_extension.as_deref() {
        Some(extension) => directory.join(format!("{}.{}", item.sha256, extension)),
        None => directory.join(&item.sha256),
    }
}

fn discover_executable(explicit: Option<&Path>) -> ProfilerResult<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return fs::canonicalize(path).map_err(|source| ProfilerError::Io {
                operation: "canonicalizing Siegfried executable",
                path: path.to_path_buf(),
                source,
            });
        }
        return Err(ProfilerError::contract_with_context(
            ErrorCode::FormatToolNotFound,
            "configured Siegfried executable does not exist",
            false,
            BTreeMap::from([("path".into(), path.to_string_lossy().into_owned())]),
        ));
    }
    let binary = if cfg!(windows) { "sf.exe" } else { "sf" };
    let mut candidates = Vec::new();
    if let Ok(current) = std::env::current_exe()
        && let Some(parent) = current.parent()
    {
        candidates.push(parent.join("tools").join("siegfried").join(binary));
        candidates.push(
            parent
                .join("resources")
                .join("tools")
                .join("siegfried")
                .join(binary),
        );
        candidates.push(parent.join(binary));
    }
    candidates.push(PathBuf::from("tools").join("siegfried").join(binary));
    if let Some(path) = find_on_path(binary) {
        candidates.push(path);
    }
    for candidate in candidates {
        if candidate.is_file() {
            return fs::canonicalize(&candidate).map_err(|source| ProfilerError::Io {
                operation: "canonicalizing discovered Siegfried executable",
                path: candidate,
                source,
            });
        }
    }
    Err(ProfilerError::contract(
        ErrorCode::FormatToolNotFound,
        "Siegfried was not found; install the pinned sidecar or pass --siegfried",
        false,
    ))
}

fn discover_signature(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    [
        parent.join("default.sig"),
        parent.join("data").join("default.sig"),
        parent.join("siegfried").join("default.sig"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(binary))
        .find(|candidate| candidate.is_file())
}

fn path_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn extract_pronom_version(identifiers: &[RawIdentifier]) -> Option<String> {
    identifiers.iter().find_map(|identifier| {
        let marker = "DROID_SignatureFile_V";
        let start = identifier.details.find(marker)? + marker.len();
        let digits = identifier.details[start..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        (!digits.is_empty()).then(|| format!("v{digits}"))
    })
}

fn normalize_errors(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => (!text.trim().is_empty()).then(|| text.clone()),
        serde_json::Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
                .join("; ");
            (!joined.is_empty()).then_some(joined)
        }
        other => Some(other.to_string()),
    }
}

fn sha256_file(path: &Path) -> ProfilerResult<String> {
    let file = File::open(path).map_err(|source| ProfilerError::Io {
        operation: "opening format tool for SHA-256",
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| ProfilerError::Io {
                operation: "hashing format tool",
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn now_text() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC3339 formatting is infallible for OffsetDateTime")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_argument_is_relative_to_siegfried_home() {
        let mut command = Command::new("sf");
        let signature = Path::new("/opt/mailvault/siegfried/default.sig");

        append_signature_arguments(&mut command, signature).expect("signature arguments");
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            arguments,
            vec!["-home", "/opt/mailvault/siegfried", "-sig", "default.sig"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_signature_argument_never_contains_absolute_backslashes() {
        let mut command = Command::new("sf.exe");
        let signature =
            Path::new(r"E:\github\mailvault-collection-profiler\tools\siegfried\default.sig");

        append_signature_arguments(&mut command, signature).expect("signature arguments");
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(arguments[0], "-home");
        assert_eq!(
            arguments[1],
            r"E:\github\mailvault-collection-profiler\tools\siegfried"
        );
        assert_eq!(arguments[2], "-sig");
        assert_eq!(arguments[3], "default.sig");
        assert!(!arguments[3].contains('\\'));
    }

    #[test]
    fn pronom_version_is_parsed_from_identifier_details() {
        let identifiers = vec![RawIdentifier {
            name: "pronom".into(),
            details: "DROID_SignatureFile_V124.xml; container-signature-20260711.xml".into(),
        }];
        assert_eq!(
            extract_pronom_version(&identifiers).as_deref(),
            Some("v124")
        );
    }

    #[test]
    fn byte_or_container_evidence_beats_extension_only() {
        let extension = FormatMatch {
            namespace: "pronom".into(),
            identifier: "fmt/1".into(),
            format_name: "A".into(),
            format_version: String::new(),
            mime_type: String::new(),
            format_class: None,
            basis: "extension match doc".into(),
            warning: String::new(),
            is_primary: false,
        };
        let byte = FormatMatch {
            basis: "byte match at 0".into(),
            identifier: "fmt/2".into(),
            ..extension.clone()
        };
        assert_eq!(choose_primary_match(&[extension, byte]), Some(1));
    }

    #[test]
    fn extension_only_match_is_not_decisive_when_signature_evidence_exists() {
        let extension = FormatMatch {
            namespace: "pronom".into(),
            identifier: "fmt/40".into(),
            format_name: "Generic Word".into(),
            format_version: String::new(),
            mime_type: "application/msword".into(),
            format_class: None,
            basis: "extension match doc".into(),
            warning: String::new(),
            is_primary: false,
        };
        let signature = FormatMatch {
            identifier: "fmt/609".into(),
            format_name: "Microsoft Word 97-2003".into(),
            basis: "container match with name WordDocument".into(),
            ..extension.clone()
        };
        let matches = [extension, signature];
        let has_signature_evidence = matches
            .iter()
            .any(|format_match| is_viable_match(format_match) && !is_extension_only(format_match));
        let decisive = matches
            .iter()
            .filter(|format_match| {
                is_viable_match(format_match)
                    && (!has_signature_evidence || !is_extension_only(format_match))
            })
            .count();
        assert_eq!(decisive, 1);
    }
}
