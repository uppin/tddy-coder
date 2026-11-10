/** Response fixtures shared by the workspace specs' in-memory backends (trimmed copy of tddy-web's). */
import { create } from "@bufbuild/protobuf";
import { GitHubUserSchema, type GitHubUser } from "tddy-client-web/gen/auth_pb";

export function aGitHubUser(overrides: Partial<GitHubUser> = {}): GitHubUser {
  return create(GitHubUserSchema, {
    login: "testuser",
    avatarUrl: "https://example.com/avatar.png",
    name: "Test User",
    id: BigInt(42),
    ...overrides,
  });
}
