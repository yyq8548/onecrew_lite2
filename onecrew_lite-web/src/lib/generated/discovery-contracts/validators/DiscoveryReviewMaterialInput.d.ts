import type { DiscoveryReviewMaterialInput } from "../types/DiscoveryReviewMaterialInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryReviewMaterialInput: ContractValidator<DiscoveryReviewMaterialInput>;
