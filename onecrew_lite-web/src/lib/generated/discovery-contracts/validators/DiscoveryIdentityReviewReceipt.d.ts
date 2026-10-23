import type { DiscoveryIdentityReviewReceipt } from "../types/DiscoveryIdentityReviewReceipt";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryIdentityReviewReceipt: ContractValidator<DiscoveryIdentityReviewReceipt>;
