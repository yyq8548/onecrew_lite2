import type { GetDiscoveryReviewInput } from "../types/GetDiscoveryReviewInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateGetDiscoveryReviewInput: ContractValidator<GetDiscoveryReviewInput>;
