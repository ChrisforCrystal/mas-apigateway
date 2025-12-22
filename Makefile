.PHONY: docker-build docker-push deploy undeploy

# Image names
IMG_CP ?= masapigateway/control-plane:latest
IMG_DP ?= masapigateway/data-plane:latest

# Build docker images
docker-build:
	# Build Control Plane (Context: control-plane/)
	docker build -f control-plane/Dockerfile -t ${IMG_CP} control-plane/
	# Build Data Plane (Context: root, because it needs ../proto)
	docker build -f data-plane/Dockerfile -t ${IMG_DP} .

# Push docker images
docker-push:
	docker push ${IMG_CP}
	docker push ${IMG_DP}

# Deploy to K8s
deploy:
	kubectl apply -f deploy/rbac.yaml
	kubectl apply -f deploy/crd.yaml
	kubectl apply -f deploy/configmap.yaml
	kubectl apply -f deploy/deployment.yaml

# Deploy test resources (TLS, Nginx, CRD Routes)
deploy-test:
	# Create TLS secret if certs exist
	-kubectl create secret tls my-tls-secret --cert=server.crt --key=server.key --dry-run=client -o yaml | kubectl apply -f -
	# Create Wasm Plugins ConfigMap
	-kubectl create configmap mas-agw-plugins --from-file=deny_all.wasm=plugins/deny-all/target/wasm32-unknown-unknown/release/deny_all.wasm --dry-run=client -o yaml | kubectl apply -f -
	# Deploy Nginx upstream
	kubectl apply -f k8s-test.yaml
	# Deploy Dynamic Route CRD
	kubectl apply -f k8s-test-crd.yaml

# Cleanup
undeploy:
	kubectl delete -f deploy/deployment.yaml
	kubectl delete -f deploy/configmap.yaml
	kubectl delete -f deploy/crd.yaml
	kubectl delete -f deploy/rbac.yaml
	kubectl delete secret my-tls-secret --ignore-not-found
