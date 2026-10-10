import { describe, expect, it } from "vitest";
import { jmesPath } from "./jmespath";

const sample = {
  users: [
    { id: 1, name: "Ada", role: "admin", active: true, score: 95, tags: ["rust", "math"] },
    { id: 2, name: "Grace", role: "admin", active: false, score: 88, tags: ["cobol"] },
    { id: 3, name: "Linus", role: "maintainer", active: true, score: 91, tags: ["c", "git"] },
  ],
  meta: {
    region: "eu-west-1",
    counts: { total: 3, active: 2 },
  },
};

describe("jmesPath", () => {
  it("resolves nested identifiers and indices", () => {
    expect(jmesPath(sample, "meta.counts.total")).toBe(3);
    expect(jmesPath(sample, "users[0].name")).toBe("Ada");
    expect(jmesPath(sample, "users[-1].name")).toBe("Linus");
  });

  it("supports wildcard and flatten projections", () => {
    expect(jmesPath(sample, "users[*].name")).toEqual(["Ada", "Grace", "Linus"]);
    expect(jmesPath(sample, "users[].tags[]")).toEqual(["rust", "math", "cobol", "c", "git"]);
    expect(jmesPath(sample, "meta.counts.*")).toEqual([3, 2]);
  });

  it("supports array slices", () => {
    expect(jmesPath(sample, "users[0:2].name")).toEqual(["Ada", "Grace"]);
    expect(jmesPath(sample, "users[::-1].name")).toEqual(["Linus", "Grace", "Ada"]);
  });

  it("evaluates filter projections with comparisons and boolean logic", () => {
    expect(jmesPath(sample, "users[?active == `true`].name")).toEqual(["Ada", "Linus"]);
    expect(jmesPath(sample, "users[?role == 'admin' && score >= `90`].name")).toEqual(["Ada"]);
    expect(jmesPath(sample, "users[?role == 'maintainer' || score < `90`].name")).toEqual([
      "Grace",
      "Linus",
    ]);
  });

  it("evaluates multi-select lists and hashes", () => {
    expect(jmesPath(sample, "users[0].[id, name]")).toEqual([1, "Ada"]);
    expect(jmesPath(sample, "users[*].{id: id, label: name}")).toEqual([
      { id: 1, label: "Ada" },
      { id: 2, label: "Grace" },
      { id: 3, label: "Linus" },
    ]);
  });

  it("evaluates pipes and built-in functions", () => {
    expect(jmesPath(sample, "users[*].score | sum(@)")).toBe(274);
    expect(jmesPath(sample, "users | sort_by(@, &score)[*].name")).toEqual([
      "Grace",
      "Linus",
      "Ada",
    ]);
    expect(jmesPath(sample, "users | max_by(@, &score).name")).toBe("Ada");
    expect(jmesPath(sample, "join(', ', users[*].name)")).toBe("Ada, Grace, Linus");
    expect(jmesPath(sample, "length(users)")).toBe(3);
  });
});
