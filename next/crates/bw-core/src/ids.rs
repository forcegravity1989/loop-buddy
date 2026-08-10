//! Opaque newtype identities. Each wraps a [`Uuid`] but is a distinct type, so a
//! `ProjectId` can never be passed where a `SessionId` is expected.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Wrap an existing UUID (e.g. one loaded from the store).
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// The nil (all-zero) id — a placeholder, handy in tests.
            pub const fn nil() -> Self {
                Self(Uuid::nil())
            }

            /// The inner UUID.
            pub const fn uuid(self) -> Uuid {
                self.0
            }

            /// Generate a fresh random id. Native only (`idgen` feature); the
            /// wasm32 keepalive build deliberately keeps RNG out of the kernel.
            #[cfg(feature = "idgen")]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        #[cfg(feature = "idgen")]
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_newtype!(
    /// Stable identity of a [`crate::model::Project`].
    ProjectId
);
id_newtype!(
    /// Stable identity of a [`crate::model::WorkflowSpec`].
    WorkflowId
);
id_newtype!(
    /// Stable identity of a [`crate::model::Session`].
    SessionId
);
id_newtype!(
    /// Stable identity of a metric (leading / lagging / stage KPI).
    MetricId
);
id_newtype!(
    /// Stable identity of a [`crate::model::Routine`].
    RoutineId
);
id_newtype!(
    /// Stable identity of a [`crate::model::SkillCard`].
    SkillId
);
id_newtype!(
    /// Stable identity of one real file belonging to an imported skill
    /// folder (T2, plan/12 §2) — a `skill_file` row (`references/mocking.md`,
    /// `agents/openai.yaml`, …). Distinct from `SkillId`: a skill has many.
    SkillFileId
);
id_newtype!(
    /// Stable identity of a [`crate::model::AgentCard`].
    AgentId
);
id_newtype!(
    /// Stable identity of a [`crate::model::CronTask`].
    CronTaskId
);
id_newtype!(
    /// Stable identity of a [`crate::model::Connector`].
    ConnectorId
);
id_newtype!(
    /// Stable identity of a [`crate::model::KnowledgeSource`].
    KnowledgeSourceId
);
id_newtype!(
    /// Stable identity of one execution of a workflow (a run record).
    WorkflowRunId
);
id_newtype!(
    /// Stable identity of a [`crate::model::Artifact`] — one registered file
    /// version (`project × path × git_commit`) in a project's workspace.
    ArtifactId
);
id_newtype!(
    /// Stable identity of an [`crate::model::Issue`].
    IssueId
);
id_newtype!(
    /// Stable identity of a `claude_conversation` row — buddy's own id for
    /// one claude CLI session (`--resume <claude_session_id>`) bound to an
    /// interactive Issue. V1 终端会话重构阶段1: 活(Issue)和会话
    /// (Conversation)解耦,会话有独立身份,可跨多次点开,不随活 Done 而
    /// 结束。和 claude CLI 的 session_id 是两个 id: 这个是 buddy 内部的稳定
    /// 行 id, claude_session_id 是 hook 回传的 claude CLI 会话 id(用于
    /// `--resume`)。
    ConversationId
);
id_newtype!(
    /// Stable identity of one execution attempt of an [`crate::IssueId`] —
    /// a `run` row (next 切片四,design-s4-runmanager.md §2.2/§7). Distinct
    /// from `WorkflowRunId` (v1 器官,workflow-scoped): this is the
    /// per-issue delivery/consultation run the vNext run manager tracks.
    /// **晚到消息的钥匙是这个编号,不是防重编号**——同一件活的两次运行
    /// 长得一模一样,按防重编号认人会把运行 A 的晚到结果记到运行 B 头上
    /// (design §4.3)。
    RunId
);
