import type { ResultStateFailure } from "../types/ResultStateFailure";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateResultStateFailure: ContractValidator<ResultStateFailure>;
