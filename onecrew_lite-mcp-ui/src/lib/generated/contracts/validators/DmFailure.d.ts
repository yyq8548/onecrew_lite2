import type { DmFailure } from "../types/DmFailure";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDmFailure: ContractValidator<DmFailure>;
