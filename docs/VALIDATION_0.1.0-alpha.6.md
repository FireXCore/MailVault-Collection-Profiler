# Alpha 6 validation contract

Alpha 4 is the trusted static baseline. Alpha 6 must additionally prove on a copied real workspace:

1. initial profile and exact-format completion;
2. immediate no-change run with no successful object reprocessing;
3. one real new-message delta;
4. duplicate attachment SHA reuse;
5. metadata-only update detection;
6. repair and retry of an unavailable object;
7. force-kill and same-run resume during snapshot, inventory, file-stat and format;
8. bounded snapshot/database growth under repeated cycles;
9. source MailVault SHA-256 unchanged;
10. cumulative result equivalent to a clean rebuild.
