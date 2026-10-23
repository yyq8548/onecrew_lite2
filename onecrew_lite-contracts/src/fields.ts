import { z } from "zod";
import { isWellFormedUnicode } from "./json.ts";

export const resultByteLength = (value: string): number => new TextEncoder().encode(value).byteLength;
export const resultIdentifier = (maximum = 128) => z.string().regex(/^[A-Za-z0-9._:-]+$/).refine(value => resultByteLength(value) <= maximum);
export const resultText = z.string().min(1).refine(value => isWellFormedUnicode(value) && resultByteLength(value) <= 8192 && /\P{White_Space}/u.test(value));
