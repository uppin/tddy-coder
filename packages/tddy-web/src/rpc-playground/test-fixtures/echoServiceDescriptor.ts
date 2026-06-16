/**
 * Binary `google.protobuf.FileDescriptorSet` fixture for `test.EchoService`.
 *
 * Built from the generated protobuf-es descriptor embedded in `echo_service_pb.ts`.
 * In @bufbuild/protobuf v2 a `GenFile` is a `DescFile`, which exposes `.proto`
 * (a `FileDescriptorProto`) and `.dependencies` (transitive `DescFile`s). We collect
 * the file and its dependencies into a `FileDescriptorSet` and serialize it, mirroring
 * what the Rust ServerReflection service returns at runtime.
 */
import { toBinary, type DescFile } from "@bufbuild/protobuf";
import {
  FileDescriptorSetSchema,
  type FileDescriptorProto,
} from "@bufbuild/protobuf/wkt";
import { file_test_echo_service } from "../../gen/test/echo_service_pb";

function collectFiles(
  file: DescFile,
  seen: Map<string, FileDescriptorProto>,
): void {
  const name = file.proto.name;
  if (seen.has(name)) {
    return;
  }
  // Dependencies first, so the set is topologically ordered (deps before dependents).
  for (const dep of file.dependencies) {
    collectFiles(dep, seen);
  }
  seen.set(name, file.proto);
}

export const ECHO_SERVICE_DESCRIPTOR_FIXTURE: Uint8Array = (() => {
  const files = new Map<string, FileDescriptorProto>();
  collectFiles(file_test_echo_service, files);
  return toBinary(FileDescriptorSetSchema, {
    $typeName: "google.protobuf.FileDescriptorSet",
    file: Array.from(files.values()),
  });
})();
