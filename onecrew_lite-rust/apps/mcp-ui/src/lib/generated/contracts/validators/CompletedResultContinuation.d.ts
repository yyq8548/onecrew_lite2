import type { CompletedResultContinuation } from "../types/CompletedResultContinuation";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateCompletedResultContinuation: ContractValidator<CompletedResultContinuation>;
