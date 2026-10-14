import type { ClaimedResultContinuation } from "../types/ClaimedResultContinuation";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateClaimedResultContinuation: ContractValidator<ClaimedResultContinuation>;
