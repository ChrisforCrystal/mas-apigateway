# Feature 003: Kubernetes Service Discovery

## 1. Background & Goal

Currently, AGW upstream clusters and endpoints must be manually configured in `config.yaml`. In a cloud-native environment, Pod IPs are ephemeral. We need AGW Control Plane to automatically discover backend service endpoints from Kubernetes.

**Goal**: Implement a Kubernetes Controller within the Control Plane that watches `Service` and `EndpointSlice` resources, translating them into AGW `Cluster` and `Endpoint` configurations dynamically.

## 2. Technical Architecture: Controller vs Operator

The user asked whether to use an "Operator" or other technology.

**Decision**: We will implement a **Native Kubernetes Controller** using `client-go` (Informers), integrated directly into the `agw-control-plane`.

- **Why not a full Operator (with CRDs) yet?**

  - For _Service Discovery_, we don't need custom resources (CRDs) yet. We are consuming native K8s `Service` and `EndpointSlice` resources.
  - A custom CRD-based Operator is useful for _Deployment_ (managing AGW's own lifecycle) or _Custom Routing_ (e.g., `AgwRoute`). We can introduce that later.
  - Embedding the Controller logic in the Control Plane is the standard pattern for Ingress Controllers (like Nginx Ingress, Traefik, Istio w/o Operator). It's simpler and more performant for data propagation.

- **Core Technology**:
  - **SDK**: `k8s.io/client-go`
  - **Pattern**: Shared Informers (List & Watch with local cache).
  - **Resource**: `discovery.k8s.io/v1 EndpointSlice` (more scalable than legacy v1/Endpoints).

## 3. User Stories

- **US.3.1**: As a user, I want to reference a K8s Service name (e.g., `my-app.default`) in my config, and have traffic routed to its healthy pods automatically.
- **US.3.2**: When I scale my application pods up/down, AGW should update its load balancing list within seconds.
- **US.3.3**: The Control Plane should be able to run both inside the cluster (In-Cluster Config) and outside (Kubeconfig) for development.

## 4. Functional Requirements

1.  **K8s Client Initialization**: Support `KUBECONFIG` (local) and `InClusterConfig` (prod).
2.  **Service Watching**: Watch `v1/Service` to map Service Names to Cluster configs and discover Target Ports.
3.  **Endpoint Watching**: Watch `discovery/v1/EndpointSlice` to find Pod IPs.
4.  **Synchronization**: Sync K8s state to the internal `ConfigSnapshot` store.
    - _Conflict Resolution_: If `config.yaml` defines a static cluster, does K8s override it?
    - _Policy_: For this feature, K8s discovered services will be added to the `Clusters` list. If a name acts as a key, we need a naming convention, e.g., `k8s/{namespace}/{service}`.

## 5. Success Criteria

- **Auto-Update**: Scaling a Deployment in K8s results in new Endpoints appearing in the Data Plane (verified via logs/metrics).
- **Connectivity**: Data Plane can route requests to Pod IPs discovered via the Control Plane.
