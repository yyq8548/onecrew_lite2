import type { DiscoveryReviewMaterial } from "../types/DiscoveryReviewMaterial";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryReviewMaterial: ContractValidator<DiscoveryReviewMaterial>;
