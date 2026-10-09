import { describe, expect, it } from "vitest";

import { expiryLabel, fromLocalInput, parseCookieHeader, parseSetCookie, toLocalInput } from "./cookies";

const NOW = Date.parse("2026-10-09T12:00:00Z");

describe("parseSetCookie", () => {
  it("defaults the domain to the host and the path to the request directory", () => {
    expect(parseSetCookie("sid=abc; HttpOnly; SameSite=lax", "http://localhost:8080/v1/auth/login", NOW)).toEqual({
      name: "sid",
      value: "abc",
      domain: "localhost",
      includeSubdomains: false,
      path: "/v1/auth",
      expires: null,
      secure: false,
      httpOnly: true,
      sameSite: "Lax",
    });
  });

  it("reads Domain, Path, Secure and expiry, Max-Age winning", () => {
    const c = parseSetCookie(
      'theme="dark"; Domain=.Acme.dev; Path=/; Secure; Expires=Wed, 21 Oct 2099 07:28:00 GMT; Max-Age=3600',
      "https://api.acme.dev/x",
      NOW,
    );
    expect(c).toMatchObject({ value: "dark", domain: "acme.dev", includeSubdomains: true, path: "/", secure: true, expires: "2026-10-09T13:00:00Z" });
    expect(parseSetCookie("a=1; Expires=Wed, 21 Oct 2099 07:28:00 GMT", "https://x.test/", NOW)?.expires).toBe("2099-10-21T07:28:00Z");
    expect(parseSetCookie("novalue", "https://x.test/")).toBeNull();
  });
});

describe("helpers", () => {
  it("splits Cookie headers", () => {
    expect(parseCookieHeader("a=1; b=x=y;c")).toEqual([
      { name: "a", value: "1" },
      { name: "b", value: "x=y" },
      { name: "c", value: "" },
    ]);
  });

  it("labels expiry", () => {
    expect(expiryLabel(null, NOW)).toBe("Session");
    expect(expiryLabel("2026-10-09T11:00:00Z", NOW)).toBe("Expired");
    expect(expiryLabel("2026-10-09T12:10:00Z", NOW)).toBe("in 10 min");
    expect(expiryLabel("2026-10-09T15:00:00Z", NOW)).toBe("in 3 h");
    expect(expiryLabel("2026-11-07T12:00:00Z", NOW)).toBe("in 29 days");
  });

  it("round-trips datetime-local values", () => {
    const iso = "2026-12-24T18:30:00Z";
    expect(fromLocalInput(toLocalInput(iso))).toBe(iso);
    expect(fromLocalInput("")).toBeNull();
  });
});
