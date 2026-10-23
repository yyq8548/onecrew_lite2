import assert from "node:assert/strict";
import test from "node:test";
import { assertOperationAvailable, OperationRetiredError, retiredOperations } from "../src/contracts/retired-operations.ts";

test("all explicitly retired operations fail with a stable compatibility error", () => {
  for (const operation of retiredOperations) {
    assert.throws(() => assertOperationAvailable(operation), (error: unknown) => {
      assert.ok(error instanceof OperationRetiredError);
      assert.equal(error.status, 410);
      assert.equal(error.code, "operation.retired");
      assert.equal(error.operation, operation);
      return true;
    });
  }
});

test("public reads, historical receipts and shared Creator review remain outside sending retirement", () => {
  for (const operation of [
    "x.profile.get", "x.post_thread.get", "telegram.community.administrators.list",
    "instagram.profile.get", "onecrew.creator.candidate.save", "onecrew.ask_user_question",
    "onecrew.outreach.action.status", "onecrew.outreach.conversation.get",
    "email.message.get", "email.thread.get", "email.draft.get", "email.template.get",
  ]) assert.doesNotThrow(() => assertOperationAvailable(operation));
});
