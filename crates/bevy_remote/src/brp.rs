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
        entity: BrpEntity,
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
        entity: BrpEntity,
    },
    Insert {
        entity: BrpEntity,
        components: BrpComponentMap,
    },
    Remove {
        entity: BrpEntity,
        components: BrpComponentNames,
    },
    Reparent {
        entity: BrpEntity,
        parent: BrpEntity,
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

#[derive(Debug)]
pub struct BrpEntity(pub Entity);

impl Serialize for BrpEntity {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let index = self.0.index();
        let generation = self.0.generation();
        let serialization = format!("{}v{}", index, generation);
        serializer.serialize_str(serialization.as_str())
    }
}

impl<'de> Deserialize<'de> for BrpEntity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const INVALID_ENTITY_SERIALIZATION: &str = "Invalid entity serialization";

        let deserialized = String::deserialize(deserializer)?;
        let (index, generation) = deserialized
            .split_once("v")
            .ok_or_else(|| serde::de::Error::custom(INVALID_ENTITY_SERIALIZATION))?;
        let index = index
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom(INVALID_ENTITY_SERIALIZATION))?;
        let generation = generation
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom(INVALID_ENTITY_SERIALIZATION))?;

        #[cfg(target_endian = "little")]
        let bits = (generation as u64) << 32 | index as u64;

        #[cfg(target_endian = "big")]
        let bits = (index as u64) << 32 | generation as u64;

        Ok(Self(Entity::from_bits(bits)))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Hash, Eq, Clone)]
pub struct BrpComponentName(pub String);

pub type BrpComponentMap = HashMap<BrpComponentName, BrpComponent>;
pub type BrpComponentOptionalMap = HashMap<BrpComponentName, Option<BrpComponent>>;
pub type BrpComponentHasMap = HashMap<BrpComponentName, bool>;

#[derive(Serialize, Deserialize, Debug)]
pub enum BrpComponent {
    #[serde(rename = "JSON")]
    Json(String),

    #[serde(rename = "RON")]
    Ron(String),
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
        entity: BrpEntity,
        components: BrpComponentMap,
    },
    Query {
        entities: BrpQueryResults,
    },
    Spawn {
        entity: BrpEntity,
    },
    Poll {
        entities: BrpQueryResults,
        watermark: BrpWatermark,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BrpQueryResult {
    pub entity: BrpEntity,
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
    ComponentInvalidAccess(String),
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
