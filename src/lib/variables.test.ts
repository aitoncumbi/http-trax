import { describe, expect, it } from "vitest";

import { buildScope, completeVar, findVars, openVarAt, resolveText, suggest } from "./variables";

const scope = buildScope({
  workspace: [{ key: "region", value: "eu", enabled: true }],
  collection: { name: "Orders API", variables: [{ key: "baseUrl", value: "http://localhost:3000", enabled: true }, { key: "apiVersion", value: "v1", enabled: true }] },
  environment: {
    id: "e",
    name: "Local",
    shared: false,
    variables: [
      { key: "baseUrl", value: "http://localhost:8080/{{apiVersion}}", enabled: true },
      { key: "apiToken", value: "s3cr3t", enabled: true, secret: true },
      { key: "off", value: "x", enabled: false },
    ],
    overrides: { orderId: "ord_841" },
  },
  runtime: { traceId: "t-1" },
});

describe("buildScope", () => {
  it("applies precedence and labels sources", () => {
    expect(scope.get("baseUrl")).toMatchObject({ source: "environment", label: "Local environment" });
    expect(scope.get("apiVersion")).toMatchObject({ source: "collection", value: "v1" });
    expect(scope.get("orderId")).toMatchObject({ source: "local" });
    expect(scope.get("traceId")).toMatchObject({ source: "runtime" });
    expect(scope.get("apiToken")?.secret).toBe(true);
    expect(scope.has("off")).toBe(false);
    expect(scope.get("$uuid")?.source).toBe("dynamic");
  });

  it("resolves nested values", () => {
    expect(resolveText("{{baseUrl}}/orders/{{ orderId }}?id={{$uuid}}&x={{nope}}", scope)).toBe(
      "http://localhost:8080/v1/orders/ord_841?id={{$uuid}}&x={{nope}}",
    );
  });
});

describe("completion", () => {
  it("finds the unfinished placeholder before the caret", () => {
    expect(openVarAt("{{base", 6)).toEqual({ start: 0, query: "base" });
    expect(openVarAt("x {{ ord", 8)).toEqual({ start: 2, query: "ord" });
    expect(openVarAt("{{baseUrl}}/a", 13)).toBeNull();
    expect(openVarAt("plain", 5)).toBeNull();
  });

  it("completes and reuses closing braces", () => {
    expect(completeVar("{{ba/orders", 4, "baseUrl")).toEqual({ text: "{{baseUrl}}/orders", caret: 11 });
    expect(completeVar("{{baseU}}/x", 7, "baseUrl")).toEqual({ text: "{{baseUrl}}/x", caret: 11 });
    expect(completeVar("{{", 2, "$uuid")).toEqual({ text: "{{$uuid}}", caret: 9 });
  });

  it("suggests prefix matches first", () => {
    const names = suggest(scope, "a", 20).map((v) => v.name);
    expect(names.slice(0, 2)).toEqual(["apiToken", "apiVersion"]);
    expect(names).toEqual(expect.arrayContaining(["baseUrl", "traceId", "$timestamp"]));
    expect(suggest(scope, "").at(-1)?.source).toBe("dynamic");
    expect(suggest(scope, "$ti").map((v) => v.name)).toEqual(["$timestamp", "$timestampMs"]);
  });

  it("lists placeholders with positions", () => {
    expect(findVars("a {{ x }} b {{y}}")).toEqual([
      { name: "x", start: 2, end: 9 },
      { name: "y", start: 12, end: 17 },
    ]);
  });
});
