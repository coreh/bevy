//! Bevy Remote Protocol (BRP) data structures and utilities.

use bevy_ecs::entity::Entity;
use bevy_utils::HashMap;
use serde::{Deserialize, Serialize};

/// A Bevy Remote Protocol (BRP) request.
#[derive(Serialize, Deserialize, Debug)]
pub struct BrpRequest {
    /// A numeric identifier for the request.
    /// Used to match requests with responses when they are delivered out of order.
    #[serde(default)]
    pub id: BrpId,

    /// The content of the request.
    #[serde(flatten)]
    pub request: BrpRequestContent,
}

/// The content of a BRP request.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "request", content = "params")]
pub enum BrpRequestContent {
    /// Request a ping response, used to test the connection.
    Ping,

    /// Request data for a specific entity.
    GetEntity {
        /// Which entity to get data for.
        entity: Entity,
        #[serde(default)]

        /// What data to get for the entity.
        /// Can also act as a filter, see [`BrpQueryData`] for more information.
        data: BrpQueryData,

        /// Additional filters that the entity must match.
        #[serde(default)]
        filter: BrpQueryFilter,
    },

    /// Request data for multiple entities, based on a query.
    QueryEntities {
        /// What data to get for the entities.
        /// Can also act as a filter, see [`BrpQueryData`] for more information.
        #[serde(default)]
        data: BrpQueryData,

        /// Additional filters that the entities must match.
        #[serde(default)]
        filter: BrpQueryFilter,
    },

    /// Request spanwing of a new entity with the given components.
    SpawnEntity {
        /// The components to spawn the entity with.
        components: BrpComponentMap,
    },

    /// Request destruction of an entity.
    DestroyEntity {
        /// The entity to destroy.
        entity: Entity,
    },

    /// Request insertion of components into an entity.
    InsertComponent {
        /// The entity to insert the components into.
        entity: Entity,

        /// The components to insert.
        components: BrpComponentMap,
    },

    /// Request removal of components from an entity.
    RemoveComponent {
        /// The entity to remove the components from.
        entity: Entity,

        /// The components to remove.
        components: BrpComponentNames,
    },

    /// Request reparenting of an entity.
    ReparentEntity {
        /// The entity to reparent.
        entity: Entity,

        /// The new parent of the entity.
        parent: Entity,
    },

    /// Similar to [`BrpRequestContent::GetEntity`] but a response is only sent when the given
    /// watermark is invalidated. (i.e. when the entities/components affected by the query change)
    PollEntities {
        /// What data to get for the entities.
        /// Can also act as a filter, see [`BrpQueryData`] for more information.
        #[serde(default)]
        data: BrpQueryData,

        /// Additional filters that the entities must match.
        #[serde(default)]
        filter: BrpQueryFilter,

        /// The watermark to poll for.
        /// If `None`, the request is equivalent to [`BrpRequestContent::QueryEntities`].
        watermark: Option<BrpWatermark>,
    },

    /// Request data for an asset.
    GetAsset {
        /// The name of the asset to get.
        name: BrpAssetName,

        /// A handle pointing to the asset, in serialized form. (e.g. `Handle<StandardMaterial>`)
        handle: BrpSerializedData,
    },

    /// Request insertion of an asset.
    InsertAsset {
        /// The name of the asset to insert. (e.g. `"StandardMaterial"`)
        name: BrpAssetName,

        /// A handle pointing to the asset, in serialized form. (e.g. `Handle<StandardMaterial>`)
        handle: BrpSerializedData,

        /// The asset to insert, in serialized form. (e.g. `StandardMaterial`)
        asset: BrpSerializedData,
    },
}

/// A list of component names.
pub type BrpComponentNames = Vec<BrpComponentName>;

/// A numeric identifier for a BRP request.
pub type BrpId = u64;

/// A watermark for a BRP poll request.
pub type BrpWatermark = u64;

/// A string representing the name of a component.
///
/// Can be both a long or a short type path. (e.g. `"bevy_transform::components::transform::Transform"`
/// or `"Transform"`)
///
/// In case of a short type path, if there are multiple types with the same short name,
/// a [`BrpError::ComponentAmbiguous`] error will be produced.
pub type BrpComponentName = String;

/// A string representing the name of an asset.
///
/// Can be both a long or a short type path. (e.g. `"bevy_pbr::pbr_material::StandardMaterial"`
/// or `"StandardMaterial"`)
///
/// In case of a short type path, if there are multiple types with the same name,
/// a [`BrpError::ComponentAmbiguous`] error will be produced.
pub type BrpAssetName = String;

/// A map of component names to serialized component data.
pub type BrpComponentMap = HashMap<BrpComponentName, BrpSerializedData>;

/// A map of component names to optional serialized component data.
pub type BrpComponentOptionalMap = HashMap<BrpComponentName, Option<BrpSerializedData>>;

/// A map of component names to boolean values, indicating their presence.
pub type BrpComponentHasMap = HashMap<BrpComponentName, bool>;

/// Data in serialized form (e.g. from a component or asset) or a special value.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BrpSerializedData {
    /// A JSON string.
    #[serde(rename = "JSON")]
    Json(String),

    /// A JSON5 string.
    #[serde(rename = "JSON5")]
    Json5(String),

    /// A RON string.
    #[serde(rename = "RON")]
    Ron(String),

    /// Produces `Default::default()` when deserialized.
    #[serde(rename = "<<Default>>")]
    Default,

    /// Represents a value that cannot be serialized/deserialized.
    #[serde(rename = "<<Unserializable>>")]
    Unserializable,
}

/// What data to fetch for [`BrpRequestContent::GetEntity`], [`BrpRequestContent::QueryEntities`] and [`BrpRequestContent::PollEntities`] requests.
///
/// Acts as a filter for the entities to fetch. For example, if `components` is provided, only entities with all the given components will be fetched.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct BrpQueryData {
    /// Required components for the entities, to fetch.
    /// Entities will be fetched only if they have all of these components.
    #[serde(default)]
    pub components: BrpComponentNames,

    /// Optional components for the entities, to fetch.
    /// Entities will be fetched even if they don't have these components.
    #[serde(default)]
    pub optional: BrpComponentNames,

    /// Optional components for the entities, to check for presence.
    #[serde(default)]
    pub has: BrpComponentNames,
}

/// Additional filters for [`BrpRequestContent::GetEntity`], [`BrpRequestContent::QueryEntities`] and [`BrpRequestContent::PollEntities`] requests.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct BrpQueryFilter {
    /// Only fetch entities that have all of these components.
    /// (The components are not fetched, only used as a filter)
    #[serde(default)]
    pub with: BrpComponentNames,

    /// Only fetch entities that don't have any of these components.
    #[serde(default)]
    pub without: BrpComponentNames,

    /// Only fetch entities that match the given predicate.
    #[serde(default)]
    pub when: BrpPredicate,
}

/// An expression that can be used to filter entities based on the values of their components.
#[derive(Serialize, Deserialize, Debug, Default)]
pub enum BrpPredicate {
    /// Always `true`.
    #[default]
    Always,

    /// `true` if all of the nested predicates are `true`.
    ///
    /// Equivalent to the logical AND operator (`&&`).
    #[serde(rename = "&&")]
    All(Vec<BrpPredicate>),

    /// `true` if any of the nested predicates are `true`.
    ///
    /// Equivalent to the logical OR operator (`||`).
    #[serde(rename = "||")]
    Any(Vec<BrpPredicate>),

    /// `true` if the nested predicate is `false`.
    ///
    /// Equivalent to the logical NOT operator (`!`).
    #[serde(rename = "!")]
    Not(Box<BrpPredicate>),

    /// `true` if the given components are equal (or more precisely `PartialEq`) to the given values.
    #[serde(rename = "==")]
    PartialEq(BrpComponentMap),
}

/// A BRP response.
#[derive(Serialize, Deserialize, Debug)]
pub struct BrpResponse {
    /// The id of the request that this response is for.
    ///
    /// Used to match requests with responses when they are delivered out of order.
    #[serde(default)]
    pub id: BrpId,

    /// The content of the response.
    #[serde(flatten)]
    pub response: BrpResponseContent,
}

/// The content of a BRP response.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "response", content = "content")]
pub enum BrpResponseContent {
    /// The request was successful, no additional data is returned.
    #[serde(rename = "OK")]
    Ok,

    /// The request was unsuccessful, an error is returned.
    Error(BrpError),

    /// The success response for a [`BrpRequestContent::GetEntity`] request.
    GetEntity {
        /// Data for the requested entity.
        entity: BrpQueryResult,
    },

    /// The success response for a [`BrpRequestContent::QueryEntities`] request.
    QueryEntities {
        /// Data for each entity that matched the query.
        entities: BrpQueryResults,
    },

    /// The success response for a [`BrpRequestContent::SpawnEntity`] request.
    SpawnEntity {
        /// The id of the entity that was spawned.
        entity: Entity,
    },

    /// The success response for a [`BrpRequestContent::PollEntities`] request.
    Poll {
        /// Data for each entity that matched the query.
        entities: BrpQueryResults,

        /// The new watermark for the query.
        watermark: BrpWatermark,
    },

    /// The success response for a [`BrpRequestContent::GetAsset`] request.
    GetAsset {
        /// The name of the asset.
        name: BrpAssetName,

        /// The handle pointing to the asset, in serialized form.
        handle: BrpSerializedData,

        /// The asset, in serialized form.
        asset: BrpSerializedData,
    },
}

/// An individual result of a BRP query. (For a single entity)
#[derive(Serialize, Deserialize, Debug)]
pub struct BrpQueryResult {
    /// The id of the entity that the data is for.
    pub entity: Entity,

    /// The required components for the entity, as specified by the query.
    #[serde(default)]
    pub components: BrpComponentMap,

    /// The optional components for the entity, as specified by the query.
    #[serde(default)]
    pub optional: BrpComponentOptionalMap,

    /// Boolean values indicating the presence of components, as specified by the query.
    #[serde(default)]
    pub has: BrpComponentHasMap,
}

/// A list of BRP query results. (For querying multiple entities)
pub type BrpQueryResults = Vec<BrpQueryResult>;

/// A BRP error.
#[derive(Serialize, Deserialize, Debug)]
pub enum BrpError {
    /// The requested entity was not found.
    EntityNotFound,

    /// The requested component was not found.
    ComponentNotFound(String),

    /// The requested component was found, but the short type path used is ambiguous.
    ComponentAmbiguous(String),

    /// The requested component was found, but the access was invalid.
    ComponentInvalidAccess(String),

    /// A component or asset in the request is missing a type id.
    MissingTypeId(String),

    /// A component or asset in the request is missing a component id.
    MissingComponentId(String),

    /// A component or asset in the request is missing a type registration.
    MissingTypeRegistration(String),

    /// A component or asset in the request is missing a `Reflect` implementation.
    MissingReflect(String),

    /// A component or asset in the request is missing a `Default` (or `ReflectDefault`) implementation.
    MissingDefault(String),

    /// A component or asset in the request is missing a `PartialEq` (or `ReflectPartialEq`) implementation.
    MissingPartialEq(String),

    /// A component or asset in the request is missing a `Serialize` (or `ReflectSerialize`) implementation.
    Serialization(String),

    /// An error occurred during deserialization of a component or asset.
    Deserialization {
        /// The type name of the affected component or asset.
        type_name: String,

        /// The error message from the underlying deserialization library.
        error: String,
    },

    /// The requested asset was not found.
    AssetNotFound(String),

    /// The request is invalid.
    InvalidRequest,

    /// The entity in the request is invalid.
    InvalidEntity,

    /// The query in the request is invalid.
    InvalidQuery,

    /// The watermark in the request is invalid.
    InvalidWatermark,

    /// An internal error occurred during processing of the request.
    InternalError,

    /// The request timed out.
    Timeout,

    /// Support for processing the request is not implemented.
    Unimplemented,

    /// Some other, potentially transport-specific error occurred.
    Other(String),
}

impl BrpResponse {
    /// Creates a new response with the given id and content.
    pub fn new(id: BrpId, response: BrpResponseContent) -> Self {
        Self { id, response }
    }

    /// Creates a new response with the given id and an error.
    pub fn from_error(id: BrpId, error: BrpError) -> Self {
        Self {
            id,
            response: BrpResponseContent::Error(error),
        }
    }
}

impl BrpPredicate {
    /// Returns an iterator over the component names in the predicate.
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
