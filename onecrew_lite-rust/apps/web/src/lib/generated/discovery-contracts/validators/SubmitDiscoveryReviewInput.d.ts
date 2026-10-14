import type { SubmitDiscoveryReviewInput } from "../types/SubmitDiscoveryReviewInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateSubmitDiscoveryReviewInput: ContractValidator<SubmitDiscoveryReviewInput>;
