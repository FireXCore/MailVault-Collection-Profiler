mod contract;
mod file_stat;
mod inventory;
mod layout;
mod preflight;
mod snapshot;

pub use file_stat::MailVaultPhysicalObjectResolver;
pub use layout::MailVaultLayout;

use std::path::Path;

use profiler_core::{
    CollectionAdapter, InventoryRequest, InventoryResult, InventorySink, InventorySource,
    PreflightReport, ProfilerResult, ProgressSink, SnapshotRequest, SnapshotResult,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct MailVaultAdapter;

impl MailVaultAdapter {
    pub const ADAPTER_VERSION: &'static str = env!("CARGO_PKG_VERSION");
}

impl CollectionAdapter for MailVaultAdapter {
    fn kind(&self) -> &'static str {
        "mailvault"
    }

    fn preflight(&self, archive_root: &Path) -> ProfilerResult<PreflightReport> {
        preflight::run_preflight(archive_root)
    }

    fn create_snapshot(
        &self,
        request: &SnapshotRequest,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<SnapshotResult> {
        snapshot::create_snapshot(request, progress)
    }
}

impl InventorySource for MailVaultAdapter {
    fn inventory(
        &self,
        request: &InventoryRequest,
        sink: &mut dyn InventorySink,
        progress: &dyn ProgressSink,
    ) -> ProfilerResult<InventoryResult> {
        inventory::run_inventory(request, sink, progress)
    }
}
