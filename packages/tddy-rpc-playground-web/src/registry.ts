/**
 * Build a protobuf descriptor registry from a serialized FileDescriptorSet (as returned
 * by the gRPC ServerReflection service) and look up methods within it.
 */
import {
  createFileRegistry,
  fromBinary,
  type DescMethod,
  type Registry,
} from "@bufbuild/protobuf";
import { FileDescriptorSetSchema } from "@bufbuild/protobuf/wkt";

/** Parse a binary `google.protobuf.FileDescriptorSet` into a queryable registry. */
export function buildRegistry(descriptorSetBytes: Uint8Array): Registry {
  const fds = fromBinary(FileDescriptorSetSchema, descriptorSetBytes);
  return createFileRegistry(fds);
}

/** Resolve a method descriptor by fully-qualified service name and method name. */
export function findMethod(
  registry: Registry,
  serviceName: string,
  methodName: string,
): DescMethod {
  const service = registry.getService(serviceName);
  if (!service) {
    throw new Error(`Service not found in registry: ${serviceName}`);
  }
  const method = service.methods.find((m) => m.name === methodName);
  if (!method) {
    throw new Error(`Method not found in service ${serviceName}: ${methodName}`);
  }
  return method;
}
