import assert from "node:assert/strict";
import test from "node:test";
import { parseApplicationJson, stringifyApplicationJson } from "../src/contracts/application-json.ts";

test("application JSON preserves full u64 as numbers on the wire and rejects lossy or malformed inputs", () => {
  const bytes = Buffer.from('{"followers":18446744073709551615,"small":42,"ratio":0.125,"negative":-9223372036854775808}');
  const value = parseApplicationJson(bytes);
  assert.deepEqual(value, { followers: 18446744073709551615n, small: 42, ratio: 0.125, negative: -9223372036854775808n });
  assert.equal(stringifyApplicationJson(value), bytes.toString());
  assert.throws(() => parseApplicationJson(Buffer.from('{"key":1,"key":2}')), /Duplicate key/);
  assert.throws(() => parseApplicationJson(Buffer.from([123, 34, 120, 34, 58, 34, 128, 34, 125])), /UTF-8/i);
  assert.throws(() => parseApplicationJson(Buffer.from('{"text":"\\ud800"}')), /Unicode/);
  assert.throws(() => parseApplicationJson(Buffer.from('{"ratio":1e400}')), /without loss/);
  assert.throws(() => stringifyApplicationJson(undefined), /not JSON/);
});
