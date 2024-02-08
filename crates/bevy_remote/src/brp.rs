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
pub enum BrpRequestContent {
    Ping,
    GetEntity {
        entity: Entity,
        #[serde(default)]
        data: BrpQueryData,
        #[serde(default)]
        filter: BrpQueryFilter,
    },
    QueryEntities {
        #[serde(default)]
        data: BrpQueryData,
        #[serde(default)]
        filter: BrpQueryFilter,
    },
    SpawnEntity {
        components: BrpComponentMap,
    },
    DestroyEntity {
        entity: Entity,
    },
    InsertComponent {
        entity: Entity,
        components: BrpComponentMap,
    },
    RemoveComponent {
        entity: Entity,
        components: BrpComponentNames,
    },
    ReparentEntity {
        entity: Entity,
        parent: Entity,
    },
    PollEntities {
        #[serde(default)]
        data: BrpQueryData,
        #[serde(default)]
        filter: BrpQueryFilter,
        watermark: Option<BrpWatermark>,
    },
    GetAsset {
        name: BrpAssetName,
        handle: BrpSerializedData,
    },
    InsertAsset {
        name: BrpAssetName,
        handle: BrpSerializedData,
        asset: BrpSerializedData,
    },
}

pub type BrpComponentNames = Vec<BrpComponentName>;

pub type BrpId = u64;

pub type BrpWatermark = u64;

pub type BrpComponentName = String;
pub type BrpAssetName = String;

pub type BrpComponentMap = HashMap<BrpComponentName, BrpSerializedData>;
pub type BrpComponentOptionalMap = HashMap<BrpComponentName, Option<BrpSerializedData>>;
pub type BrpComponentHasMap = HashMap<BrpComponentName, bool>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BrpSerializedData {
    #[serde(rename = "JSON")]
    Json(String),

    #[serde(rename = "JSON5")]
    Json5(String),

    #[serde(rename = "RON")]
    Ron(String),

    #[serde(rename = "<<Default>>")]
    Default,

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
    GetEntity {
        entity: BrpQueryResult,
    },
    QueryEntities {
        entities: BrpQueryResults,
    },
    SpawnEntity {
        entity: Entity,
    },
    Poll {
        entities: BrpQueryResults,
        watermark: BrpWatermark,
    },
    GetAsset {
        name: BrpAssetName,
        handle: BrpSerializedData,
        asset: BrpSerializedData,
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
    ComponentInvalidAccess(String),
    MissingTypeId(String),
    MissingComponentId(String),
    MissingTypeRegistration(String),
    MissingReflect(String),
    MissingDefault(String),
    MissingPartialEq(String),
    Serialization(String),
    Deserialization { type_name: String, error: String },
    AssetNotFound(String),
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
