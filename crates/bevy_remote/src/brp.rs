use bevy_ecs::entity::Entity;
use bevy_utils::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct BrpRequest {
    #[serde(default)]
    pub id: BrpId,

    #[serde(flatten)]
    pub request: BrpRequestContent,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "request", content = "params")]
#[serde(rename_all = "UPPERCASE")]
pub enum BrpRequestContent {
    Ping,
    Get {
        entity: Entity,
        components: BrpComponentNames,
    },
    Query {
        #[serde(default)]
        data: BrpQueryData,
        #[serde(default)]
        filter: BrpQueryFilter,
    },
    Spawn {
        components: BrpComponentMap,
    },
    Destroy {
        entity: Entity,
    },
    Insert {
        entity: Entity,
        components: BrpComponentMap,
    },
    Remove {
        entity: Entity,
        components: BrpComponentNames,
    },
    Reparent {
        entity: Entity,
        parent: Entity,
    },
    Poll {
        #[serde(default)]
        data: BrpQueryData,
        #[serde(default)]
        filter: BrpQueryFilter,
        watermark: Option<BrpWatermark>,
    },
}

pub type BrpComponentNames = Vec<BrpComponentName>;

pub type BrpId = u64;

pub type BrpWatermark = u64;

pub type BrpComponentName = String;

pub type BrpComponentMap = HashMap<BrpComponentName, BrpComponent>;
pub type BrpComponentOptionalMap = HashMap<BrpComponentName, Option<BrpComponent>>;
pub type BrpComponentHasMap = HashMap<BrpComponentName, bool>;

#[derive(Serialize, Deserialize, Debug)]
pub enum BrpComponent {
    #[serde(rename = "JSON")]
    Json(String),

    #[serde(rename = "RON")]
    Ron(String),

    #[serde(rename = "<<Unserializable>>")]
    Unserializable,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct BrpQueryData {
    #[serde(default)]
    pub components: BrpComponentNames,
    #[serde(default)]
    pub optional: BrpComponentNames,
    #[serde(default)]
    pub has: BrpComponentNames,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct BrpQueryFilter {
    #[serde(default)]
    pub with: BrpComponentNames,
    #[serde(default)]
    pub without: BrpComponentNames,
    #[serde(default)]
    pub when: BrpPredicate,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum BrpPredicate {
    #[default]
    Always,
    #[serde(rename = "&&")]
    All(Vec<BrpPredicate>),
    #[serde(rename = "||")]
    Any(Vec<BrpPredicate>),
    #[serde(rename = "!")]
    Not(Box<BrpPredicate>),
    #[serde(rename = "==")]
    PartialEq(BrpComponentMap),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BrpResponse {
    #[serde(default)]
    pub id: BrpId,

    #[serde(flatten)]
    pub response: BrpResponseContent,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "response", content = "content")]
pub enum BrpResponseContent {
    #[serde(rename = "OK")]
    Ok,
    Error(BrpError),
    Get {
        entity: Entity,
        components: BrpComponentMap,
    },
    Query {
        entities: BrpQueryResults,
    },
    Spawn {
        entity: Entity,
    },
    Poll {
        entities: BrpQueryResults,
        watermark: BrpWatermark,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BrpQueryResult {
    pub entity: Entity,
    #[serde(default)]
    pub components: BrpComponentMap,
    #[serde(default)]
    pub optional: BrpComponentOptionalMap,
    #[serde(default)]
    pub has: BrpComponentHasMap,
}

pub type BrpQueryResults = Vec<BrpQueryResult>;

#[derive(Serialize, Deserialize, Debug)]
pub enum BrpError {
    EntityNotFound,
    ComponentNotFound(String),
    ComponentAmbiguous(String),
    ComponentMissingTypeId(String),
    ComponentMissingTypeRegistration(String),
    ComponentMissingReflect(String),
    ComponentMissingPartialEq(String),
    ComponentInvalidAccess(String),
    ComponentDeserialization(String),
    ComponentSerialization(String),
    InvalidRequest,
    InvalidEntity,
    InvalidQuery,
    InvalidWatermark,
    InternalError,
    Timeout,
    Unimplemented,
    Other(String),
}

impl BrpResponse {
    pub fn new(id: BrpId, response: BrpResponseContent) -> Self {
        Self { id, response }
    }

    pub fn from_error(id: BrpId, error: BrpError) -> Self {
        Self {
            id,
            response: BrpResponseContent::Error(error),
        }
    }
}

impl BrpPredicate {
    pub fn iter(&self) -> Box<dyn Iterator<Item = &BrpComponentName> + '_> {
        match self {
            BrpPredicate::Always => Box::from(std::iter::empty()),
            BrpPredicate::All(predicates) => Box::from(predicates.iter().flat_map(|p| p.iter())),
            BrpPredicate::Any(predicates) => Box::from(predicates.iter().flat_map(|p| p.iter())),
            BrpPredicate::Not(predicate) => Box::from(predicate.iter()),
            BrpPredicate::PartialEq(components) => Box::from(components.keys()),
        }
    }
}
