import type { DiscoveryIdentityGroupsSnapshot } from "../types/DiscoveryIdentityGroupsSnapshot";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateDiscoveryIdentityGroupsSnapshot: ContractValidator<DiscoveryIdentityGroupsSnapshot>;
