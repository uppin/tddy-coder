/**
 * The PR's conversation, threaded (`#workqueue` 4/14): a reply sits under the comment it answers, so a
 * review thread reads as one unit; a conversation comment is a thread of one.
 *
 * Test ids: `session-pr-thread-<root id>`, `session-pr-comment-<id>`.
 */
import React from "react";
import type { PullRequestComment } from "tddy-client-web/gen/worktree_pb";

export interface CommentThreadsProps {
  comments: PullRequestComment[];
}

/**
 * Group `comments` into threads in arrival order. A comment whose `inReplyToId` names no comment in the
 * list (a reply whose parent is not part of this view) starts a thread of its own.
 */
export function threadComments(comments: PullRequestComment[]): PullRequestComment[][] {
  const byId = new Map(comments.map((comment) => [comment.id, comment]));
  const rootOf = (comment: PullRequestComment): bigint => {
    let current = comment;
    const seen = new Set<bigint>();
    while (current.inReplyToId !== 0n && byId.has(current.inReplyToId) && !seen.has(current.id)) {
      seen.add(current.id);
      current = byId.get(current.inReplyToId)!;
    }
    return current.id;
  };
  const threads = new Map<bigint, PullRequestComment[]>();
  for (const comment of comments) {
    const root = rootOf(comment);
    const thread = threads.get(root);
    if (thread) thread.push(comment);
    else threads.set(root, [comment]);
  }
  return [...threads.values()];
}

function Comment({ comment, reply }: { comment: PullRequestComment; reply: boolean }) {
  return (
    <div
      data-testid={`session-pr-comment-${comment.id}`}
      className={reply ? "ml-4 border-l border-border pl-2" : undefined}
    >
      <div>
        <span className="text-foreground">{comment.author}</span>
        {comment.reviewState !== "" ? <span> · {comment.reviewState}</span> : null}
        {comment.path !== "" ? <span> · {comment.path}</span> : null}
      </div>
      <p className="whitespace-pre-wrap text-foreground">{comment.body}</p>
    </div>
  );
}

export function CommentThreads({ comments }: CommentThreadsProps) {
  return (
    <div className="flex flex-col gap-3">
      {threadComments(comments).map(([root, ...replies]) => (
        <div key={root.id} data-testid={`session-pr-thread-${root.id}`} className="flex flex-col gap-1">
          <Comment comment={root} reply={false} />
          {replies.map((reply) => (
            <Comment key={reply.id} comment={reply} reply />
          ))}
        </div>
      ))}
    </div>
  );
}
