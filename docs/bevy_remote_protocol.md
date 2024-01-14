# Bevy Remote Protocol (BRP)

The Bevy Remote Protocol (BRP) is a transport-agnostic and serialization-agnostic protocol for communication between a Bevy application (acting as a server) and a remote (or local) client. It's a request/response-style protocol (similar to HTTP), meaning that every communication is always initiated by the client, and the server only responds to the client's requests.

## Example Uses

- A editor/inspector, allowing the user to inspect and modify the Bevy application's state in real time, both locally and remotely;
- “Gameshark“-style cheats, allowing the user to modify a game's state in real time;
- JS/HTML-based UI interacting with an embedded Bevy application running in a browser;
- Non-Bevy/Rust applications (e.g. a C++/Python/Java application) interacting with embedded Bevy modules;
- Multiplayer clients, connected to a headless Bevy application running on a server.

## Possible Transports

- Network Sockets; (TCP/UDP/QUIC)
- WebSockets;
- HTTP;
- IPC; (Unix sockets, named pipes, windows messages, mach ports, etc.)
- Shared Memory + Semaphores;
- WASM; (via JS bindings)
- Some other form of FFI; (e.g. from C, Java, Python, etc.)
- Some other form of RPC. (e.g. gRPC, Cap'n Proto, etc.)

## Possible Serializations

- JSON, other text-based formats;
- Protobuf, Flatbuffers, other binary formats;
- Anything else supported by serde, really;
- JS objects exposed via WASM/JS bindings;
- FFI structs.

## Ordering

Requests are processed by the server asynchronously and may produce out of order responses, depending on their complexity, available resources on the server and other factors. (e.g. a request that asks the server to save a screenshot of the current frame to disk might take longer than a request that asks the server to return a simple numeric value) To accomodate for this, each request is assigned a unique numerical id, and each response includes the id of the request it's responding to. The client can then use this id to match responses to requests. (e.g. via a hash table)

Requests that result in internal side effects to the server (e.g. modifying a component, creating an entity, etc.) are guaranteed to be processed in the order they were received, so that the server's state is always consistent with the client's expectation, and requests can build on top of each other on a fully deterministic basis.

## Sessions

Multiple clients can connect to the same server at the same time, resulting in multiple sessions. Request ids are scoped to each specific session, so that requests from different sessions can't be mixed up. (e.g. if two clients send requests with id 1, the server will produce two responses with id 1, one for each client)

## Polling

To allow for reactive updates in the client as a result of changes in the server in this request-response style protocol, the client can _poll_ the server for changes, via requests that run indefinetely until a desired condition is met. (e.g. a request that is only resolved when the entity list changes)

To avoid missing changes that take place between the server sending a response and the client polling again, polling requests should include a “watermark“, an opaque value that represents the last known instance of the desired condition being met, so that the server may immediately resolve the request if the condition has since been met.

## Non-Goals

As a tradeoff for simplicity, ease of implementation and universality, BRP is deliberately _not_ concerned with minimizing memory usage, processing or bandwidth, or preserving Rust borrow semantics. As a result, all data is copied, potentially multiple times between the client and the server, and responses are potentially very large.

Clients should be aware of this and be judicious about the amount of data they request from the server, and the frequency at which they poll for changes. In the future, the protocol may be extended with more efficient alternatives for specific use cases, by streaming data, sending data out of band, introducing cursors, etc.

Usecases that require updating thousands of entities per frame, (e.g. “modding“) are probably better served by a more specialized/custom implementation.

Another non-goal is to guarantee transactional consistency between multiple requests from multiple concurrent sessions. (e.g. if two clients send requests that modify the same entity at the same time, the server may intersperse them in any order, and the result may be inconsistent) In the future, the protocol may be extended with a way to group requests together, (e.g. `BEGIN`/`END` requests) so that they are processed atomically, but this is not a priority at the moment.

## Supported Requests

### `PING` Request

Used to test if the server is alive and responding.

### `GET` Request

Queries the server for the value of the given component(s) in a given entity;

#### Parameters

- `entity`: The entity to query;
- `components`: The components to query.

### `QUERY` Request

Queries the server for entities matching a given set of components and filters.

#### Parameters

- `data`: The set of components to be included in the response; (Also aff
  - `components`: Components that must be present in the entities;
  - `optional`: Components that may or may not be present in the entities;
  - `has`: Components that must be present as boolean values in the entities (indicating whether they are present or not);
- `filter`: A filter to be applied to the entities;
  - `with`: A list of components that must be present in the entities.
  - `without`: A list of components that must not be present in the entities;

### `SPAWN` Request

Spawns a new entity with the given components.

#### Parameters

- `components`: The components to be added to the new entity.

### `DESTROY` Request

Destroys an entity, removing all of its components.

#### Parameters

- `entity`: The entity to be destroyed.

### `INSERT` Request

Inserts a set of components into an entity. Replace any existing components with the same type.

#### Parameters

- `entity`: The entity to insert the components into;
- `components`: The components to be inserted.

### `REMOVE` Request

Removes a set of components from an entity. Any components that are not present in the entity are ignored, and the request is still considered successful.

#### Parameters

- `entity`: The entity to remove the components from;
- `components`: The components to be removed;

### `REPARENT` Request

Changes the parent an entity, adding it to the given parent's children list and removing it from its previous parent's children list. (Atomically)

#### Parameters

- `entity`: The entity to reparent;
- `parent`: The new parent of the entity.

### `POLL` Request

Queries the server for changes in the set of entities matching a given set of components and filters.

#### Parameters

- `data`: The set of components to be included in the response; (Also aff
  - `components`: Components that must be present in the entities;
  - `option`: Components that may or may not be present in the entities;
  - `has`: Components that must be present as boolean values in the entities (indicating whether they are present or not);
- `filter`: A filter to be applied to the entities;
  - `without`: A list of components that must not be present in the entities;
  - `with`: A list of components that must be present in the entities;
  - `changed`: A list of components that must have changed since the last poll;
- `watermark`: An opaque value that represents the last known instance of changes in the entities matching the given components and filters.

## Examples

> **Note:** The following examples use JSON for the sake of simplicity, but any other supported serialization format could be used instead.
>
> The initial handshake between the client and the server is also not shown in the examples, as it's transport-specific.

### Find all root entities

```json5
// Client -> Server
{
  "id": 1,
  "request": "QUERY",
  "params": {
    "data": {
      "components": ["Name"]
    },
    "filter": {
      "without": {
        "components": ["Parent"]
      }
    }
  }
}

// Server -> Client
{
  "id": 1,
  "response": {
    "entities": [
      { "id": "1v0", "Name": { "JSON": "\"Camera\"" } }, // Notice the nested serialization.
      { "id": "2v0", "Name": { "JSON": "\"Light\"" } },  // This is needed because the current serialization format
      { "id": "3v0", "Name": { "JSON": "\"Player\"" } }, // might not support representing arbitrary types, and to
                                                         // facilitate the use of `ReflectDeserialize` on the server side.
    ]
  }
}
```

### Update the position of an entity

```json5
// Client -> Server
{
  "id": 2,
  "request": "INSERT",
  "params": {
    "entity": "3v0",
    "components": {
      "Position": {
        "JSON": "{ \"x\": 1.0, \"y\": 2.0, \"z\": 3.0}"
      }
    }
  }
}

// Server -> Client
{
  "id": 2,
  "response": {
    "status": "OK"
  }
}
```

### Get the position of an entity

```json5
// Client -> Server
{
  "id": 3,
  "request": "GET",
  "params": {
    "entity": "3v0",
    "components": ["Position"]
  }
}

// Server -> Client
{
  "id": 3,
  "response": {
    "components": {
      "Position": {
        "JSON": "{ \"x\": 1.0, \"y\": 2.0, \"z\": 3.0}"
      }
    }
  }
}
```

### Remove a component from an entity

```json5
// Client -> Server
{
  "id": 3,
  "request": "REMOVE",
  "params": {
    "entity": "3v0",
    "components": ["Player"]
  }
}

// Server -> Client
{
  "id": 3,
  "response": {
    "status": "OK"
  }
}
```

### Poll for changes in the entity list

```json5
// Client -> Server
{
  "id": 4,
  "request": "POLL",
  "params": {
    "data": {
      "components": ["Name"]
    },
    "filter": {
      "without": {
        "components": ["Parent"]
      }
    },
    "watermark": null // This is the first poll, so there's no watermark yet
                      // so the server will always respond with the current state
  }
}

// Server -> Client
{
  "id": 4,
  "response": {
    "entities": [
      { "id": "1v0", "Name": { "JSON": "\"Camera\"" } },
      { "id": "2v0", "Name": { "JSON": "\"Light\"" } },
      { "id": "3v0", "Name": { "JSON": "\"Player\"" } },
    ],
    "watermark": "<some opaque value>" // Must be included in the next poll,
                                       // to avoid missing changes in the interregnum
  }
}
```
