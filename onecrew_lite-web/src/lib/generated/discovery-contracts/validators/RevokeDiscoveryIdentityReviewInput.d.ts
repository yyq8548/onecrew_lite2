import type { RevokeDiscoveryIdentityReviewInput } from "../types/RevokeDiscoveryIdentityReviewInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateRevokeDiscoveryIdentityReviewInput: ContractValidator<RevokeDiscoveryIdentityReviewInput>;
