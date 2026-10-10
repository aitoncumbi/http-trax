import { describe, expect, it } from "vitest";

import { fieldLabel, fieldOptions, teamChanges } from "./review";
import type { SyncReport } from "./types";

const DIFF = `diff --git a/collections/orders/create-order.req.yaml b/collections/orders/create-order.req.yaml
@@ -4,6 +4,11 @@ seq: 2
 method: POST
 url: '{{baseUrl}}/v1/orders'
+params:
+- key: dryRun
+  value: 'true'
 headers:
 - key: Authorization
   value: Bearer {{token}}
+- key: "Idempotency-Key"
+  value: '{{$uuid}}'
 body:
   mode: json
`;

describe("fieldOptions", () => {
  it("offers the request's headers and params from the diff next to the fixed fields", () => {
    const fields = fieldOptions(DIFF);
    expect(fields).toContain("headers.Authorization");
    expect(fields).toContain("headers.Idempotency-Key");
    expect(fields).toContain("params.dryRun");
    expect(fields.slice(0, 3)).toEqual(["general", "url", "method"]);
  });

  it("ignores keys outside headers and params", () => {
    expect(fieldOptions(" variables:\n - key: baseUrl\n")).not.toContain("variables.baseUrl");
  });
});

describe("fieldLabel", () => {
  it("names fields the way the request editor does", () => {
    expect(fieldLabel("headers.Authorization")).toBe("Header Authorization");
    expect(fieldLabel("params.dryRun")).toBe("Param dryRun");
    expect(fieldLabel("scripts.postResponse")).toBe("Tests");
    expect(fieldLabel("scripts.preRequest")).toBe("Pre-request script");
    expect(fieldLabel("url")).toBe("URL");
    expect(fieldLabel("general")).toBe("General");
  });
});

describe("teamChanges", () => {
  const report = (extra: Partial<SyncReport>): SyncReport => ({
    state: "behind",
    ahead: 0,
    behind: 2,
    changes: 0,
    pulled: 0,
    checkedAt: "2026-10-10T12:00:00Z",
    ...extra,
  });

  it("names who changed which requests", () => {
    expect(teamChanges(report({ authors: ["Grace Hopper"], changedRequests: ["Create order", "Refund order"] }))).toBe(
      "Grace Hopper changed Create order, Refund order",
    );
  });

  it("shortens long lists", () => {
    expect(
      teamChanges(report({ authors: ["Ada", "Grace", "Linus"], changedRequests: ["A", "B", "C", "D", "E"] })),
    ).toBe("Ada, Grace and 1 more changed A, B, C and 2 more");
  });

  it("falls back to the team when authors are unknown", () => {
    expect(teamChanges(report({ changedRequests: ["Local (env)"] }))).toBe("Your team changed Local (env)");
  });
});
