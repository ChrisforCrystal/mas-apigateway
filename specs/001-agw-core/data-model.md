# Data Model: AGW Core

**Status**: Draft
**Source**: `proto/agw.proto`

## Entities

### Node

Represents a running Data Plane instance.

- **id** (string, required): Unique identifier (UUID or hostname).
- **region** (string, optional): Logical grouping (e.g., "us-east-1").
- **version** (string, required): Semantic version of the running binary.

### ConfigSnapshot

Represents a point-in-time configuration state pushed to the Data Plane.

- **version_id** (string, required): Opaque hash or timestamp to verify sync state.
- **payload** (future): Will contain Listener, Route, and Cluster definitions.

## Relationships

- **Control Plane** 1 : N **Data Plane Nodes**
- **Control Plane** pushes **ConfigSnapshot** -> **Data Plane Node** (1-way stream)
