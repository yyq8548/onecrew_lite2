import { z } from "zod";
export { IntegrationError } from "../../../devices/errors";
export const MAIL_PREFIX = "/api/desktop/integrations/mail/v1";
export const MAIL_PROTOCOL = "onecrew.mail";
export const MAIL_VERSION = 1;
export const connectionIdSchema = z.string().refine((value) => value.startsWith("conn_") && z.string().uuid().safeParse(value.slice(5)).success);
