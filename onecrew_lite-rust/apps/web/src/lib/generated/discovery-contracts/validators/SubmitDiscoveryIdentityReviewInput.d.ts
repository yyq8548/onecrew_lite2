import type { SubmitDiscoveryIdentityReviewInput } from "../types/SubmitDiscoveryIdentityReviewInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateSubmitDiscoveryIdentityReviewInput: ContractValidator<SubmitDiscoveryIdentityReviewInput>;
