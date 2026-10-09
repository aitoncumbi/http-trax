// http-trax script prelude.
//
// Evaluated once per script run, before the user's script. Everything the
// user can touch lives on the `trax` object (with a Postman-flavoured `pm`
// alias). The sandbox has no file, network or process access: the only way
// data leaves is the JSON document returned by `__traxFinish()`.
//
// `__traxInput` is defined by the host right before this file is evaluated.

var __traxState = (function (input) {
  "use strict";

  var logs = [];
  var tests = [];
  var pending = [];
  var envChanges = {};
  var varChanges = {};
  var skip = false;

  var env = Object.assign({}, input.environment || {});
  var vars = Object.assign({}, input.variables || {});
  var collectionVars = Object.assign({}, input.collectionVariables || {});

  // ---------------------------------------------------------------- helpers

  function str(value) {
    if (value === undefined || value === null) return "";
    if (typeof value === "string") return value;
    try {
      return JSON.stringify(value);
    } catch (_) {
      return String(value);
    }
  }

  function fmt(value) {
    if (typeof value === "string") return JSON.stringify(value);
    if (value === undefined) return "undefined";
    if (typeof value === "function") return "[Function]";
    try {
      var out = JSON.stringify(value);
      return out === undefined ? String(value) : out;
    } catch (_) {
      return String(value);
    }
  }

  function errorMessage(e) {
    if (e === undefined || e === null) return "Unknown error";
    if (typeof e === "string") return e;
    if (e.message !== undefined) return (e.name && e.name !== "Error" ? e.name + ": " : "") + e.message;
    return String(e);
  }

  function deepEqual(a, b) {
    if (Object.is(a, b)) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    var ka = Object.keys(a);
    var kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (var i = 0; i < ka.length; i++) {
      if (!Object.prototype.hasOwnProperty.call(b, ka[i])) return false;
      if (!deepEqual(a[ka[i]], b[ka[i]])) return false;
    }
    return true;
  }

  function getPath(obj, path) {
    var parts = Array.isArray(path) ? path : String(path).split(".");
    var cur = obj;
    for (var i = 0; i < parts.length; i++) {
      if (cur === null || cur === undefined) return { found: false };
      if (typeof cur !== "object" && typeof cur !== "function") return { found: false };
      if (!(parts[i] in Object(cur))) return { found: false };
      cur = cur[parts[i]];
    }
    return { found: true, value: cur };
  }

  function log(level) {
    return function () {
      var parts = [];
      for (var i = 0; i < arguments.length; i++) {
        var a = arguments[i];
        parts.push(typeof a === "string" ? a : fmt(a));
      }
      logs.push({ level: level, message: parts.join(" ") });
    };
  }

  var consoleObj = {
    log: log("log"),
    info: log("info"),
    warn: log("warn"),
    error: log("error"),
    debug: log("debug"),
  };

  // ------------------------------------------------------------- base64

  var B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

  function utf8Encode(s) {
    var out = "";
    for (var i = 0; i < s.length; i++) {
      var c = s.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
        var d = s.charCodeAt(i + 1);
        if (d >= 0xdc00 && d <= 0xdfff) {
          c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
          i++;
        }
      }
      if (c < 0x80) out += String.fromCharCode(c);
      else if (c < 0x800) out += String.fromCharCode(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000)
        out += String.fromCharCode(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else
        out += String.fromCharCode(
          0xf0 | (c >> 18),
          0x80 | ((c >> 12) & 63),
          0x80 | ((c >> 6) & 63),
          0x80 | (c & 63)
        );
    }
    return out;
  }

  function btoa(input) {
    var s = String(input);
    var out = "";
    for (var i = 0; i < s.length; i += 3) {
      var a = s.charCodeAt(i);
      var b = s.charCodeAt(i + 1);
      var c = s.charCodeAt(i + 2);
      if (a > 255 || b > 255 || c > 255) {
        throw new Error("btoa: string contains characters outside of the Latin1 range");
      }
      var n = (a << 16) | ((b || 0) << 8) | (c || 0);
      out += B64.charAt((n >> 18) & 63) + B64.charAt((n >> 12) & 63);
      out += i + 1 < s.length ? B64.charAt((n >> 6) & 63) : "=";
      out += i + 2 < s.length ? B64.charAt(n & 63) : "=";
    }
    return out;
  }

  function atob(input) {
    var s = String(input).replace(/[\s=]/g, "");
    var out = "";
    var buffer = 0;
    var bits = 0;
    for (var i = 0; i < s.length; i++) {
      var idx = B64.indexOf(s.charAt(i));
      if (idx < 0) throw new Error("atob: invalid base64 input");
      buffer = (buffer << 6) | idx;
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        out += String.fromCharCode((buffer >> bits) & 255);
      }
    }
    return out;
  }

  function uuid() {
    var hex = "0123456789abcdef";
    var out = "";
    for (var i = 0; i < 36; i++) {
      if (i === 8 || i === 13 || i === 18 || i === 23) out += "-";
      else if (i === 14) out += "4";
      else if (i === 19) out += hex.charAt((Math.random() * 4) | 8);
      else out += hex.charAt((Math.random() * 16) | 0);
    }
    return out;
  }

  // ------------------------------------------------------------- expect

  function AssertionError(message) {
    this.name = "AssertionError";
    this.message = message;
  }
  AssertionError.prototype = Object.create(Error.prototype);
  AssertionError.prototype.constructor = AssertionError;

  function typeName(v) {
    if (v === null) return "null";
    if (Array.isArray(v)) return "array";
    return typeof v;
  }

  function makeExpect(actual, negate) {
    function check(pass, message) {
      if (negate ? pass : !pass) {
        throw new AssertionError((negate ? "Expected not: " : "Expected: ") + message);
      }
      return api;
    }

    var api = {};

    // Jest-style matchers.
    api.toBe = function (v) {
      return check(Object.is(actual, v), fmt(actual) + " to be " + fmt(v));
    };
    api.toEqual = function (v) {
      return check(deepEqual(actual, v), fmt(actual) + " to equal " + fmt(v));
    };
    api.toStrictEqual = api.toEqual;
    api.toBeTruthy = function () {
      return check(!!actual, fmt(actual) + " to be truthy");
    };
    api.toBeFalsy = function () {
      return check(!actual, fmt(actual) + " to be falsy");
    };
    api.toBeDefined = function () {
      return check(actual !== undefined, "value to be defined");
    };
    api.toBeUndefined = function () {
      return check(actual === undefined, fmt(actual) + " to be undefined");
    };
    api.toBeNull = function () {
      return check(actual === null, fmt(actual) + " to be null");
    };
    api.toContain = function (v) {
      var pass = false;
      if (typeof actual === "string") pass = actual.indexOf(String(v)) !== -1;
      else if (Array.isArray(actual)) {
        for (var i = 0; i < actual.length && !pass; i++) pass = deepEqual(actual[i], v);
      } else if (actual && typeof actual === "object" && v && typeof v === "object") {
        pass = Object.keys(v).every(function (k) {
          return deepEqual(actual[k], v[k]);
        });
      }
      return check(pass, fmt(actual) + " to contain " + fmt(v));
    };
    api.toHaveProperty = function (path, value) {
      var res = getPath(actual, path);
      if (arguments.length > 1) {
        return check(
          res.found && deepEqual(res.value, value),
          fmt(actual) + " to have property " + fmt(path) + " equal to " + fmt(value)
        );
      }
      return check(res.found, fmt(actual) + " to have property " + fmt(path));
    };
    api.toHaveLength = function (n) {
      var len = actual === null || actual === undefined ? undefined : actual.length;
      return check(len === n, "length " + fmt(len) + " to be " + fmt(n));
    };
    api.toBeGreaterThan = function (n) {
      return check(actual > n, fmt(actual) + " to be greater than " + fmt(n));
    };
    api.toBeGreaterThanOrEqual = function (n) {
      return check(actual >= n, fmt(actual) + " to be >= " + fmt(n));
    };
    api.toBeLessThan = function (n) {
      return check(actual < n, fmt(actual) + " to be less than " + fmt(n));
    };
    api.toBeLessThanOrEqual = function (n) {
      return check(actual <= n, fmt(actual) + " to be <= " + fmt(n));
    };
    api.toMatch = function (re) {
      var r = re instanceof RegExp ? re : new RegExp(String(re));
      return check(r.test(String(actual)), fmt(actual) + " to match " + String(r));
    };
    api.toBeOneOf = function (list) {
      var pass = (list || []).some(function (x) {
        return deepEqual(x, actual);
      });
      return check(pass, fmt(actual) + " to be one of " + fmt(list));
    };
    api.toBeTypeOf = function (t) {
      return check(typeName(actual) === t, fmt(actual) + " to be of type " + t);
    };
    api.toThrow = function () {
      var threw = false;
      try {
        actual();
      } catch (_) {
        threw = true;
      }
      return check(threw, "function to throw");
    };

    // Chai-style aliases so common Postman snippets work unchanged.
    api.equal = api.equals = api.eq = api.toBe;
    api.eql = api.deep_equal = api.toEqual;
    api.include = api.includes = api.contain = api.contains = api.toContain;
    api.above = api.gt = api.greaterThan = api.toBeGreaterThan;
    api.least = api.gte = api.toBeGreaterThanOrEqual;
    api.below = api.lt = api.lessThan = api.toBeLessThan;
    api.most = api.lte = api.toBeLessThanOrEqual;
    api.property = api.toHaveProperty;
    api.lengthOf = api.length = api.toHaveLength;
    api.match = api.toMatch;
    api.oneOf = api.toBeOneOf;
    api.a = api.an = function (t) {
      return check(typeName(actual) === String(t).toLowerCase(), fmt(actual) + " to be a " + t);
    };

    var chains = ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "same", "does", "still", "deep"];
    chains.forEach(function (name) {
      Object.defineProperty(api, name, {
        get: function () {
          return api;
        },
      });
    });
    Object.defineProperty(api, "not", {
      get: function () {
        return makeExpect(actual, !negate);
      },
    });
    var flags = {
      ok: function () {
        return check(!!actual, fmt(actual) + " to be ok");
      },
      true: function () {
        return check(actual === true, fmt(actual) + " to be true");
      },
      false: function () {
        return check(actual === false, fmt(actual) + " to be false");
      },
      null: function () {
        return check(actual === null, fmt(actual) + " to be null");
      },
      undefined: function () {
        return check(actual === undefined, fmt(actual) + " to be undefined");
      },
      exist: function () {
        return check(actual !== null && actual !== undefined, "value to exist");
      },
      empty: function () {
        var empty =
          actual === "" ||
          (Array.isArray(actual) && actual.length === 0) ||
          (actual && typeof actual === "object" && Object.keys(actual).length === 0);
        return check(!!empty, fmt(actual) + " to be empty");
      },
    };
    Object.keys(flags).forEach(function (name) {
      Object.defineProperty(api, name, { get: flags[name] });
    });

    return api;
  }

  function expect(actual) {
    return makeExpect(actual, false);
  }

  // -------------------------------------------------------------- tests

  function test(name, fn) {
    var entry = { name: String(name), passed: true, error: null };
    tests.push(entry);
    try {
      var result = typeof fn === "function" ? fn() : undefined;
      if (result && typeof result.then === "function") {
        pending.push(
          result.then(
            function () {},
            function (e) {
              entry.passed = false;
              entry.error = errorMessage(e);
            }
          )
        );
      }
    } catch (e) {
      entry.passed = false;
      entry.error = errorMessage(e);
    }
  }
  test.skip = function (name) {
    tests.push({ name: String(name), passed: true, skipped: true, error: null });
  };

  // ---------------------------------------------------------- variables

  function scope(store, changes, readOnly) {
    return {
      get: function (key) {
        return Object.prototype.hasOwnProperty.call(store, key) ? store[key] : undefined;
      },
      has: function (key) {
        return Object.prototype.hasOwnProperty.call(store, key);
      },
      set: function (key, value) {
        if (readOnly) throw new Error("these variables are read-only from scripts");
        store[key] = str(value);
        changes[key] = store[key];
      },
      unset: function (key) {
        if (readOnly) throw new Error("these variables are read-only from scripts");
        delete store[key];
        changes[key] = null;
      },
      clear: function () {
        if (readOnly) throw new Error("these variables are read-only from scripts");
        Object.keys(store).forEach(function (k) {
          delete store[k];
          changes[k] = null;
        });
      },
      toObject: function () {
        return Object.assign({}, store);
      },
      replaceIn: function (template) {
        return interpolate(template);
      },
    };
  }

  function lookup(key) {
    if (Object.prototype.hasOwnProperty.call(vars, key)) return vars[key];
    if (Object.prototype.hasOwnProperty.call(env, key)) return env[key];
    if (Object.prototype.hasOwnProperty.call(collectionVars, key)) return collectionVars[key];
    return undefined;
  }

  function interpolate(template) {
    return String(template).replace(/\{\{\s*([^{}\s]+)\s*\}\}/g, function (m, key) {
      var v = lookup(key);
      return v === undefined ? m : v;
    });
  }

  // ------------------------------------------------------------ request

  var req = input.request || {};
  var request = {
    method: String(req.method || "GET"),
    url: String(req.url || ""),
    body: req.body === undefined || req.body === null ? null : String(req.body),
    name: (input.info && input.info.requestName) || "",
  };
  var headerList = (req.headers || []).map(function (h) {
    return { key: String(h.key), value: String(h.value) };
  });

  function findHeader(list, name) {
    var lower = String(name).toLowerCase();
    for (var i = 0; i < list.length; i++) {
      if (list[i].key.toLowerCase() === lower) return i;
    }
    return -1;
  }

  request.headers = {
    get: function (name) {
      var i = findHeader(headerList, name);
      return i === -1 ? undefined : headerList[i].value;
    },
    has: function (name) {
      return findHeader(headerList, name) !== -1;
    },
    set: function (name, value) {
      var i = findHeader(headerList, name);
      if (i === -1) headerList.push({ key: String(name), value: str(value) });
      else headerList[i].value = str(value);
    },
    add: function (h, value) {
      if (h && typeof h === "object") headerList.push({ key: String(h.key), value: str(h.value) });
      else headerList.push({ key: String(h), value: str(value) });
    },
    upsert: function (h) {
      request.headers.set(h.key, h.value);
    },
    remove: function (name) {
      headerList = headerList.filter(function (x) {
        return x.key.toLowerCase() !== String(name).toLowerCase();
      });
    },
    toObject: function () {
      var o = {};
      headerList.forEach(function (h) {
        o[h.key] = h.value;
      });
      return o;
    },
    all: function () {
      return headerList.map(function (h) {
        return { key: h.key, value: h.value };
      });
    },
  };
  request.setHeader = request.headers.set;
  request.removeHeader = request.headers.remove;

  // ----------------------------------------------------------- response

  var response = null;
  if (input.response) {
    var res = input.response;
    var resHeaders = (res.headers || []).map(function (h) {
      return { key: String(h.key), value: String(h.value) };
    });
    var parsed;
    var parsedOk = false;
    response = {
      status: res.status,
      code: res.status,
      statusText: res.statusText || "",
      time: res.timeMs || 0,
      responseTime: res.timeMs || 0,
      size: res.size || 0,
      body: res.body || "",
      text: function () {
        return res.body || "";
      },
      json: function () {
        if (!parsedOk) {
          parsed = JSON.parse(res.body || "");
          parsedOk = true;
        }
        return parsed;
      },
      headers: {
        get: function (name) {
          var i = findHeader(resHeaders, name);
          return i === -1 ? undefined : resHeaders[i].value;
        },
        has: function (name) {
          return findHeader(resHeaders, name) !== -1;
        },
        toObject: function () {
          var o = {};
          resHeaders.forEach(function (h) {
            o[h.key] = h.value;
          });
          return o;
        },
        all: function () {
          return resHeaders.slice();
        },
      },
    };
    // pm.response.to.have.status(200) / pm.response.to.be.ok
    var respTo = {
      have: {
        status: function (code) {
          if (typeof code === "number") expect(res.status).toBe(code);
          else expect(res.statusText).toBe(code);
        },
        header: function (name, value) {
          var present = findHeader(resHeaders, name) !== -1;
          if (!present) throw new AssertionError("Expected: response to have header " + fmt(name));
          if (arguments.length > 1) expect(response.headers.get(name)).toBe(value);
        },
        body: function (value) {
          if (arguments.length === 0) expect(res.body || "").not.toBe("");
          else expect(res.body || "").toBe(value);
        },
        jsonBody: function (path, value) {
          var body = response.json();
          if (arguments.length === 0) return;
          if (arguments.length === 1) expect(body).toHaveProperty(path);
          else expect(body).toHaveProperty(path, value);
        },
      },
      be: {},
    };
    Object.defineProperty(respTo.be, "ok", {
      get: function () {
        if (!(res.status >= 200 && res.status < 300)) {
          throw new AssertionError("Expected: response status " + res.status + " to be 2xx");
        }
      },
    });
    Object.defineProperty(respTo.be, "success", {
      get: function () {
        return respTo.be.ok;
      },
    });
    Object.defineProperty(respTo.be, "json", {
      get: function () {
        response.json();
      },
    });
    response.to = respTo;
  }

  // ---------------------------------------------------------------- api

  var info = input.info || {};

  var trax = {
    phase: input.phase,
    info: {
      requestName: info.requestName || "",
      requestId: info.requestId || "",
      collectionName: info.collectionName || "",
      environmentName: info.environmentName || "",
      eventName: input.phase === "pre-request" ? "prerequest" : "test",
    },
    request: request,
    response: response,
    env: scope(env, envChanges, false),
    vars: scope(vars, varChanges, false),
    collectionVars: scope(collectionVars, {}, true),
    variables: {
      get: lookup,
      has: function (k) {
        return lookup(k) !== undefined;
      },
      set: function (k, v) {
        vars[k] = str(v);
        varChanges[k] = vars[k];
      },
      unset: function (k) {
        delete vars[k];
        varChanges[k] = null;
      },
      replaceIn: interpolate,
      toObject: function () {
        return Object.assign({}, collectionVars, env, vars);
      },
    },
    test: test,
    expect: expect,
    skipRequest: function () {
      skip = true;
    },
    uuid: uuid,
    btoa: btoa,
    atob: atob,
    base64: {
      encode: function (s) {
        return btoa(utf8Encode(String(s)));
      },
      decode: function (s) {
        var bin = atob(s);
        try {
          return decodeURIComponent(
            bin
              .split("")
              .map(function (c) {
                return "%" + ("00" + c.charCodeAt(0).toString(16)).slice(-2);
              })
              .join("")
          );
        } catch (_) {
          return bin;
        }
      },
    },
  };

  // Postman-compatible alias. Only the commonly used surface is mapped.
  var pm = {
    info: trax.info,
    request: request,
    response: response,
    environment: trax.env,
    globals: trax.vars,
    collectionVariables: trax.collectionVars,
    variables: trax.variables,
    test: test,
    expect: expect,
  };

  return {
    trax: trax,
    pm: pm,
    console: consoleObj,
    expect: expect,
    btoa: btoa,
    atob: atob,
    error: null,
    pending: pending,
    finish: function () {
      return JSON.stringify({
        request: {
          method: request.method,
          url: request.url,
          headers: headerList,
          body: request.body,
        },
        environmentChanges: envChanges,
        variableChanges: varChanges,
        tests: tests,
        logs: logs,
        skipRequest: skip,
        error: this.error,
      });
    },
  };
})(__traxInput);

function __traxRun(source) {
  var state = __traxState;
  var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  var fn;
  try {
    fn = new AsyncFunction("trax", "pm", "console", "expect", "btoa", "atob", source);
  } catch (e) {
    state.error = (e && e.name ? e.name + ": " : "") + (e && e.message !== undefined ? e.message : String(e));
    return;
  }
  fn(state.trax, state.pm, state.console, state.expect, state.btoa, state.atob)
    .then(function () {
      return Promise.all(state.pending);
    })
    .catch(function (e) {
      state.error = (e && e.name ? e.name + ": " : "") + (e && e.message !== undefined ? e.message : String(e));
    });
}

function __traxFinish() {
  return __traxState.finish();
}
