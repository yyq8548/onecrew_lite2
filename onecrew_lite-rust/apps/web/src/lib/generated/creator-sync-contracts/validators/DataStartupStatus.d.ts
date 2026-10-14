import type { DataStartupStatus } from "../types/DataStartupStatus";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDataStartupStatus: ContractValidator<DataStartupStatus>;
