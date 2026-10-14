import { z } from 'zod';
import { youtubeOperations, youtubeUrlSchema } from './youtube.js';

export const platformOperations = {
  ...youtubeOperations,
} as const;

const platformOperationNames = Object.keys(platformOperations) as [
  keyof typeof platformOperations,
  ...(keyof typeof platformOperations)[],
];
export const platformOperationSchema = z.enum(platformOperationNames);

export const supportedPlatformUrlSchema = z
  .string()
  .url()
  .refine((value) => youtubeUrlSchema.safeParse(value).success, 'Supported platform URL required');

export const getPlatformOperation = (operation: string) =>
  platformOperations[operation as keyof typeof platformOperations];
