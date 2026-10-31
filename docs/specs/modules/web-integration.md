# Web integration and generic installer ownership (PLAN-890)

Man owns CLI/project/dependency management, generic process orchestration and
prepare/verify/swap/rollback installation. HTTP runtime, policy/observation and
service/client generation are owned by the fixed auto-web dependency.
[Web canonical source](../../../../auto-web/docs/specs/project.md) describes that domain.

`auto service` keeps VM/Rust service-only behavior; daemon `auto serve` keeps its
old meaning. Man composes the Web adapter at the host point and propagates typed
launch across its VM worker. Original stdio/exit/ready/shutdown, ordinary language
execution and generic process/child ownership remain. HTTP generation delegates
to Web; IPC/Tauri and non-HTTP adapters keep their original owners.

Web bundles a byte-exact Git-provenanced immutable artifact of Man's generic
installer, not a new editable implementation. Managed source/inventory/version/
resource/producer validation precedes mutation. Explicit invalid or adjacent
managed SDKs cannot borrow ambient sources. Valid service generation accepts the
Web minimum or Man Core/Full shape; UI generation still requires Full.
Prepare/verify/swap failure preserves old valid install/workspace and user data;
recovery proof includes the next actual runtime read. Historical old architecture
verification is its real Core shape, not an old UI/Full promise.

Owner checks retain `cargo t`, `cargo tcore`, `cargo tc`, `cargo ti` semantics and
frozen minima, resource groups and conditions. The language four-owner gate binds
source/lock/resource/tool identity and actual selected/passed IDs, including
consumer execution; list-only or internally consistent omissions cannot certify it.

Cargo/Ninja build execution preserves its argv, working directory, inherited
environment/console streams and original exit status. On Windows it owns the
compiler Job and explicitly terminates remaining owned auxiliaries after the
build root exits; cleanup errors propagate. Other platforms retain their
foreground process group and terminal signal behavior. Workload descendants
are never admitted through the compiler-preparation exception.

Reviewed implementation cohort: language29cf44480, Webe01232b, UI9aad12c,
Man cb4ab3d, Downc3464d0. Rebased/docs descendant mappings and the actual final
landed cohort, real frozen gates and recovery/guard status live in
[central PLAN-890](../../../../auto-lang/docs/plans/archive/890-auto-web-independent-repository-delivery.md).
Original SDK217/two ignored, historical45 distribution and14Core recovery proofs
retain their original source bounds; a current aggregate requires actual T-12 proof.
No remote publication or new management/CLI API is implied.

## 最终交付记录（2026-10-31）

r7冻结四仓门与发行已通过，实际合入、主干rebase及用户免追加测试范围，统一见语言仓 `docs/plans/archive/890-auto-web-independent-repository-delivery.md`。历史producer pins只标识实际执行来源，不冒称后续主干更新已重测。

> PLAN-107：由 docs/web-integration.md 迁入 canonical Spec；职责及历史证据边界保持，旧路径仅转引。
