import type { DiscoveryReviewReceipt } from "../types/DiscoveryReviewReceipt";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryReviewReceipt: ContractValidator<DiscoveryReviewReceipt>;
