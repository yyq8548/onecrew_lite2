import type { CreatorSyncStatus } from "../types/CreatorSyncStatus";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateCreatorSyncStatus: ContractValidator<CreatorSyncStatus>;
