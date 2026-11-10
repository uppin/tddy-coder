import { describe, expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  StartPassphraseProvider,
  StartPassphrasePrompt,
  type StartPassphrasePromptProps,
} from "./StartPassphrasePrompt";

const promptProps: StartPassphrasePromptProps = { instanceId: "host-a", active: true, sessionToken: "tok" };

function HostsDialog({ instanceId, active, sessionToken }: StartPassphrasePromptProps) {
  return <span>{`asks ${instanceId} active=${String(active)} as ${sessionToken}`}</span>;
}

describe("StartPassphrasePrompt", () => {
  test("mounts the provided prompt with the start's host, activity and token", () => {
    const html = renderToStaticMarkup(
      <StartPassphraseProvider prompt={HostsDialog}>
        <StartPassphrasePrompt {...promptProps} />
      </StartPassphraseProvider>,
    );
    expect(html).toBe("<span>asks host-a active=true as tok</span>");
  });

  test("refuses to render without a provider instead of silently dropping the question", () => {
    expect(() => renderToStaticMarkup(<StartPassphrasePrompt {...promptProps} />)).toThrow(
      "StartPassphrasePrompt needs a StartPassphraseProvider above it",
    );
  });
});
