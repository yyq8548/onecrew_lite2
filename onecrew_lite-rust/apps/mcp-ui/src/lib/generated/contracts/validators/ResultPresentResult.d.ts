import type { ResultPresentResult } from "../types/ResultPresentResult";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateResultPresentResult: ContractValidator<ResultPresentResult>;
