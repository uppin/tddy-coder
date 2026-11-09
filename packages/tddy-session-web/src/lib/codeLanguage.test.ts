import { describe, expect, it } from "bun:test";

import { codeLanguageForPath } from "./codeLanguage";

describe("codeLanguageForPath", () => {
  it("maps a Rust file to the rust Prism language", () => {
    expect(codeLanguageForPath("src/main.rs")).toEqual("rust");
  });

  it("maps TypeScript files to the tsx Prism language", () => {
    expect(codeLanguageForPath("src/app.ts")).toEqual("tsx");
    expect(codeLanguageForPath("src/App.tsx")).toEqual("tsx");
  });

  it("maps a Python file to the python Prism language", () => {
    expect(codeLanguageForPath("scripts/run.py")).toEqual("python");
  });

  it("maps a JSON file to the json Prism language", () => {
    expect(codeLanguageForPath("package.json")).toEqual("json");
  });

  it("maps YAML files (both extensions) to the yaml Prism language", () => {
    expect(codeLanguageForPath("config.yaml")).toEqual("yaml");
    expect(codeLanguageForPath("config.yml")).toEqual("yaml");
  });

  it("normalizes an uppercase extension", () => {
    expect(codeLanguageForPath("Main.RS")).toEqual("rust");
  });

  it("returns null for a file with no recognized extension", () => {
    expect(codeLanguageForPath("LICENSE")).toBeNull();
    expect(codeLanguageForPath("Makefile")).toBeNull();
  });

  it("returns null for Markdown, which the markdown renderer handles instead", () => {
    expect(codeLanguageForPath("README.md")).toBeNull();
  });
});
