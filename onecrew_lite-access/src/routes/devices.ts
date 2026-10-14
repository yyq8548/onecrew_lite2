import { Hono, type Context } from "hono";
import { z } from "zod";
import {
  deleteDevice,
  getDeviceRevocationTarget,
  getDeviceStatus,
  listDevices,
} from "../devices/service";
import { revokeAccessDeviceLifecycle } from "../devices/access-device-lifecycle";
import { DEVICE_ID_PATTERN } from "../devices/ids";
import { hasJsonContentType } from "../auth/request";
import { authJsonResponse, loadSafeSession } from "../auth/session";
import type { AppEnv } from "../types/bindings";

const revokeSchema = z.object({}).strict();

async function readJson(request: Request) {
  try {
    return await request.json();
  } catch {
    return undefined;
  }
}

async function owner(context: Context<AppEnv>) {
  const result = await loadSafeSession(
    context.get("auth"),
    context.req.raw.headers,
  );
  return {
    headers: result.headers,
    userId: result.session?.user.id ?? null,
  };
}

async function authorizedOwner(context: Context<AppEnv>) {
  const session = await owner(context);
  if (!session.userId) {
    return {
      response: authJsonResponse(
        { error: "authentication_required" as const },
        401,
        session.headers,
      ),
      session: null,
    } as const;
  }
  return {
    response: null,
    session: { headers: session.headers, userId: session.userId },
  } as const;
}

function validDeviceId(context: Context<AppEnv>) {
  const deviceId = context.req.param("deviceId");
  return typeof deviceId === "string" && DEVICE_ID_PATTERN.test(deviceId)
    ? deviceId
    : null;
}

function revocationFailure(
  result: { revocation: "invalid" | "not_found" | "revoked" | "unavailable" },
  headers: Headers,
) {
  if (result.revocation === "unavailable") {
    return authJsonResponse(
      { error: "device_revocation_unavailable" as const },
      503,
      headers,
    );
  }
  if (result.revocation !== "revoked") {
    return authJsonResponse(
      { error: "device_revocation_conflict" as const },
      409,
      headers,
    );
  }
  return null;
}

export const deviceRoutes = new Hono<AppEnv>()
  .get("/devices", async (context) => {
    const authorization = await authorizedOwner(context);
    if (!authorization.session) return authorization.response;
    const cursorValue = context.req.query("cursor");
    const cursor = cursorValue === undefined ? null : cursorValue;
    if (cursor !== null && !DEVICE_ID_PATTERN.test(cursor)) {
      return authJsonResponse(
        { error: "invalid_device_cursor" as const },
        400,
        authorization.session.headers,
      );
    }
    const page = await listDevices(
      context.env.DB,
      authorization.session.userId,
      cursor,
    );
    if (!page) {
      return authJsonResponse(
        { error: "invalid_device_cursor" as const },
        400,
        authorization.session.headers,
      );
    }
    return authJsonResponse(
      page,
      200,
      authorization.session.headers,
    );
  })
  .get("/devices/:deviceId/status", async (context) => {
    const authorization = await authorizedOwner(context);
    if (!authorization.session) return authorization.response;
    const deviceId = validDeviceId(context);
    const device = deviceId
      ? await getDeviceStatus(
          context.env.DB,
          authorization.session.userId,
          deviceId,
        )
      : null;
    if (!device) {
      return authJsonResponse(
        { error: "device_not_found" as const },
        404,
        authorization.session.headers,
      );
    }
    return authJsonResponse({ device }, 200, authorization.session.headers);
  })
  .post("/devices/:deviceId/revoke", async (context) => {
    const authorization = await authorizedOwner(context);
    if (!authorization.session) return authorization.response;
    if (!hasJsonContentType(context.req.raw.headers)) {
      return authJsonResponse(
        { error: "invalid_device_request" as const },
        400,
        authorization.session.headers,
      );
    }
    const parsed = revokeSchema.safeParse(await readJson(context.req.raw));
    if (!parsed.success) {
      return authJsonResponse(
        { error: "invalid_device_request" as const },
        400,
        authorization.session.headers,
      );
    }
    const deviceId = validDeviceId(context);
    const target = deviceId
      ? await getDeviceRevocationTarget(
          context.env.DB,
          authorization.session.userId,
          deviceId,
        )
      : null;
    if (!deviceId || !target) {
      return authJsonResponse(
        { error: "device_not_found" as const },
        404,
        authorization.session.headers,
      );
    }
    const result = await revokeAccessDeviceLifecycle(context.env, {
      deviceId,
      pairingId: target.pairingId,
      userId: authorization.session.userId,
    });
    const failure = revocationFailure(result, authorization.session.headers);
    if (failure) return failure;
    return authJsonResponse({ device: result.device }, 200, authorization.session.headers);
  })
  .delete("/devices/:deviceId", async (context) => {
    const authorization = await authorizedOwner(context);
    if (!authorization.session) return authorization.response;
    const deviceId = validDeviceId(context);
    const target = deviceId
      ? await getDeviceRevocationTarget(
          context.env.DB,
          authorization.session.userId,
          deviceId,
        )
      : null;
    if (!deviceId || !target) {
      return authJsonResponse(
        { error: "device_not_found" as const },
        404,
        authorization.session.headers,
      );
    }
    if (target.status === "active" || target.status === "pending") {
      return authJsonResponse(
        { error: "device_must_be_revoked" as const },
        409,
        authorization.session.headers,
      );
    }
    if (target.consumed) {
      const failure = revocationFailure(
        await revokeAccessDeviceLifecycle(context.env, {
          deviceId,
          pairingId: target.pairingId,
          userId: authorization.session.userId,
        }),
        authorization.session.headers,
      );
      if (failure) return failure;
    }
    const result = await deleteDevice(
      context.env.DB,
      authorization.session.userId,
      deviceId,
    );
    if (result.kind === "not_found") {
      return authJsonResponse(
        { error: "device_not_found" as const },
        404,
        authorization.session.headers,
      );
    }
    if (result.kind === "must_revoke") {
      return authJsonResponse(
        { error: "device_must_be_revoked" as const },
        409,
        authorization.session.headers,
      );
    }
    return authJsonResponse(
      { deletedDeviceId: result.deviceId },
      200,
      authorization.session.headers,
    );
  });
