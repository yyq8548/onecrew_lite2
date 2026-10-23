import type { ResultSnapshot } from "../types/ResultSnapshot";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateResultSnapshot: ContractValidator<ResultSnapshot>;
