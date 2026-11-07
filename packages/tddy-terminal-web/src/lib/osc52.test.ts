import { describe, expect, it } from "bun:test";
import { createOsc52Recognizer, OSC52_MAX_DECODED_BYTES } from "./osc52";

const ESC = "\x1b";
const BEL = "\x07";
const ST = `${ESC}\\`;

function base64(text: string): string {
  return Buffer.from(text, "utf8").toString("base64");
}

function osc52(selection: string, payload: string, terminator = BEL): string {
  return `${ESC}]52;${selection};${payload}${terminator}`;
}

/** A recognizer and every text it copied. */
function aRecognizer(options?: Parameters<typeof createOsc52Recognizer>[1]) {
  const copies: string[] = [];
  const feed = createOsc52Recognizer((text) => copies.push(text), options);
  return { feed, copies };
}

describe("createOsc52Recognizer", () => {
  it("copies the text of a write terminated by BEL", () => {
    const { feed, copies } = aRecognizer();

    feed(`before${osc52("c", base64("hello"))}after`);

    expect(copies).toEqual(["hello"]);
  });

  it("copies the text of a write terminated by ST", () => {
    const { feed, copies } = aRecognizer();

    feed(osc52("c", base64("hello"), ST));

    expect(copies).toEqual(["hello"]);
  });

  it("copies a write with an empty selection parameter", () => {
    const { feed, copies } = aRecognizer();

    feed(osc52("", base64("hello")));

    expect(copies).toEqual(["hello"]);
  });

  it("recognizes a write split at every byte boundary exactly once", () => {
    const sequence = new TextEncoder().encode(`x${osc52("c", base64("split me"))}y`);
    for (let cut = 1; cut < sequence.length; cut += 1) {
      const { feed, copies } = aRecognizer();

      feed(sequence.slice(0, cut));
      feed(sequence.slice(cut));

      expect(copies).toEqual(["split me"]);
    }
  });

  it("decodes multibyte UTF-8", () => {
    const { feed, copies } = aRecognizer();

    feed(osc52("c", base64("žąsis 🦆")));

    expect(copies).toEqual(["žąsis 🦆"]);
  });

  it("ignores a read and copies nothing", () => {
    const { feed, copies } = aRecognizer();

    feed(osc52("c", "?"));

    expect(copies).toEqual([]);
  });

  it("ignores an invalid base64 payload", () => {
    const { feed, copies } = aRecognizer();

    feed(osc52("c", "not base64 at all!!"));

    expect(copies).toEqual([]);
  });

  it("ignores a payload over the size cap", () => {
    const { feed, copies } = aRecognizer({ maxDecodedBytes: 4 });

    feed(osc52("c", base64("12345")));

    expect(copies).toEqual([]);
  });

  it("caps payloads at one MiB by default", () => {
    expect(OSC52_MAX_DECODED_BYTES).toBe(1024 * 1024);
  });

  it("ignores other OSC sequences such as a window title", () => {
    const { feed, copies } = aRecognizer();

    feed(`${ESC}]0;my title${BEL}${ESC}]2;other${ST}`);

    expect(copies).toEqual([]);
  });

  it("copies two writes in one chunk, in order", () => {
    const { feed, copies } = aRecognizer();

    feed(`${osc52("c", base64("one"))}${osc52("c", base64("two"))}`);

    expect(copies).toEqual(["one", "two"]);
  });
});
