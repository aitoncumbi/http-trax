import { describe, expect, it } from "vitest";

import { jsonPath, JsonPathError } from "./jsonpath";

const store = {
  store: {
    book: [
      { category: "reference", author: "Nigel Rees", title: "Sayings of the Century", price: 8.95 },
      { category: "fiction", author: "Evelyn Waugh", title: "Sword of Honour", price: 12.99 },
      { category: "fiction", author: "Herman Melville", title: "Moby Dick", isbn: "0-553-21311-3", price: 8.99 },
      { category: "fiction", author: "J. R. R. Tolkien", title: "The Lord of the Rings", isbn: "0-395-19395-8", price: 22.99 },
    ],
    bicycle: { color: "red", price: 19.95 },
  },
  "odd key": [1, 2, 3],
};

describe("jsonPath", () => {
  it("walks keys, indexes and wildcards", () => {
    expect(jsonPath(store, "$.store.book[0].title")).toEqual(["Sayings of the Century"]);
    expect(jsonPath(store, "store.book[-1].author")).toEqual(["J. R. R. Tolkien"]);
    expect(jsonPath(store, "$['odd key'][1]")).toEqual([2]);
    expect(jsonPath(store, "$.store.book[*].author")).toHaveLength(4);
    expect(jsonPath(store, "$.store.*")).toHaveLength(2);
    expect(jsonPath(store, "$.store.book[0,2].price")).toEqual([8.95, 8.99]);
    expect(jsonPath(store, "$.store.book.length")).toEqual([4]);
  });

  it("slices", () => {
    expect(jsonPath(store, "$.store.book[1:3].price")).toEqual([12.99, 8.99]);
    expect(jsonPath(store, "$.store.book[-2:].price")).toEqual([8.99, 22.99]);
    expect(jsonPath(store, "$['odd key'][::-1]")).toEqual([3, 2, 1]);
  });

  it("descends recursively", () => {
    expect(jsonPath(store, "$..price")).toEqual([8.95, 12.99, 8.99, 22.99, 19.95]);
    expect(jsonPath(store, "$..book[2].title")).toEqual(["Moby Dick"]);
  });

  it("filters", () => {
    expect(jsonPath(store, "$.store.book[?(@.price < 10)].title")).toEqual(["Sayings of the Century", "Moby Dick"]);
    expect(jsonPath(store, "$..book[?(@.isbn)].title")).toEqual(["Moby Dick", "The Lord of the Rings"]);
    expect(jsonPath(store, "$..book[?(@.category == 'fiction' && @.price > 20)].title")).toEqual(["The Lord of the Rings"]);
    expect(jsonPath(store, "$..book[?(@.author =~ /^h/i || @.price == 8.95)].title")).toEqual(["Sayings of the Century", "Moby Dick"]);
    expect(jsonPath(store, "$..book[?(!@.isbn)]")).toHaveLength(2);
  });

  it("returns the root for an empty path and reports syntax errors", () => {
    expect(jsonPath(store, "")).toEqual([store]);
    expect(jsonPath(store, "$.nope.deeper")).toEqual([]);
    expect(() => jsonPath(store, "$.store.book[")).toThrow(JsonPathError);
    expect(() => jsonPath(store, "$.store.book[?(@.x =~ /(/)]")).toThrow(JsonPathError);
  });
});
