import type { GetDiscoveryIdentityReviewInput } from "../types/GetDiscoveryIdentityReviewInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateGetDiscoveryIdentityReviewInput: ContractValidator<GetDiscoveryIdentityReviewInput>;
