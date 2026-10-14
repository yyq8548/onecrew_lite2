import { z } from "zod";

const accessUserIdSchema = z.string().min(1).max(128);

export const AccessUserDirectoryRequestSchema = z.object({
  userIds: z.array(accessUserIdSchema).min(1).max(100),
}).strict().refine(
  (value) => new Set(value.userIds).size === value.userIds.length,
  { message: "userIds must be unique" },
);

export const AccessUserDirectoryUserSchema = z.object({
  userId: accessUserIdSchema,
  email: z.string().email().max(254),
  bindingIp: z.string().regex(/^[0-9A-Fa-f:.]{3,64}$/).nullable(),
  bindingLocation: z.object({
    country: z.string().max(8).nullable(),
    region: z.string().max(128).nullable(),
    city: z.string().max(128).nullable(),
    colo: z.string().max(8).nullable(),
  }).strict(),
}).strict();

export const AccessUserDirectoryResultSchema = z.object({
  users: z.array(AccessUserDirectoryUserSchema).max(100),
}).strict();

export type AccessUserDirectoryRequest = z.infer<typeof AccessUserDirectoryRequestSchema>;
export type AccessUserDirectoryUser = z.infer<typeof AccessUserDirectoryUserSchema>;
export type AccessUserDirectoryResult = z.infer<typeof AccessUserDirectoryResultSchema>;

export interface AccessUserDirectoryService {
  lookupUsers(input: AccessUserDirectoryRequest): Promise<AccessUserDirectoryResult>;
}

export function normalizeAccessUserDirectoryResult(value: unknown) {
  const parsed = AccessUserDirectoryResultSchema.safeParse(value);
  return parsed.success ? parsed.data : { users: [] };
}
