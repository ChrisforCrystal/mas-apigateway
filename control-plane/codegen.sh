#!/bin/bash
set -e

# Generate Go code
protoc \
  --proto_path=../proto \
  --go_out=pkg/proto --go_opt=paths=source_relative \
  --go-grpc_out=pkg/proto --go-grpc_opt=paths=source_relative \
  agw.proto
