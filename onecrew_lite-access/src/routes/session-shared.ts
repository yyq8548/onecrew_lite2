import { authJsonResponse, safeUser } from "../auth/session";

export async function readJson(request: Request) {
  try {
    return await request.json();
  } catch {
    return undefined;
  }
}

export async function projectAuthenticatedUser(
  response: Response,
  errorCode: string,
) {
  if (!response.ok) {
    return authJsonResponse({ error: errorCode }, response.status, response.headers);
  }

  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return authJsonResponse(
      { error: "internal_server_error" },
      500,
      response.headers,
    );
  }

  const user =
    typeof body === "object" && body !== null && "user" in body
      ? (body as { user: unknown }).user
      : null;
  if (
    typeof user !== "object" ||
    user === null ||
    typeof (user as Record<string, unknown>).email !== "string" ||
    typeof (user as Record<string, unknown>).id !== "string" ||
    typeof (user as Record<string, unknown>).name !== "string"
  ) {
    return authJsonResponse(
      { error: "internal_server_error" },
      500,
      response.headers,
    );
  }

  const record = user as Record<string, unknown>;
  const image = record.image;
  if (image !== undefined && image !== null && typeof image !== "string") {
    return authJsonResponse(
      { error: "internal_server_error" },
      500,
      response.headers,
    );
  }

  return authJsonResponse(
    {
      ok: true,
      user: safeUser({
        email: record.email as string,
        id: record.id as string,
        image: image as string | null | undefined,
        name: record.name as string,
      }),
    },
    200,
    response.headers,
  );
}

export async function projectSimpleSuccess(
  response: Response,
  errorCode: string,
) {
  if (!response.ok) {
    return authJsonResponse({ error: errorCode }, response.status, response.headers);
  }
  return authJsonResponse({ ok: true }, 200, response.headers);
}
