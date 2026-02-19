use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

define_id!(
    EntityId,
    "Typed wrapper for entity UUIDs in the knowledge graph."
);
define_id!(
    ClaimId,
    "Typed wrapper for claim UUIDs in the knowledge graph."
);
define_id!(
    RelationshipId,
    "Typed wrapper for relationship UUIDs in the knowledge graph."
);
define_id!(
    AssessmentId,
    "Typed wrapper for assessment UUIDs in the assessment store."
);
define_id!(InvestigationId, "Typed wrapper for investigation UUIDs.");
define_id!(WorkOrderId, "Typed wrapper for work order UUIDs.");
