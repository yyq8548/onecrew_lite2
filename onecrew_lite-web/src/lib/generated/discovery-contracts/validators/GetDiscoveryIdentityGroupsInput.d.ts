import type { GetDiscoveryIdentityGroupsInput } from "../types/GetDiscoveryIdentityGroupsInput";
export type ContractValidator<T> = ((value: unknown) => value is T) & { errors?: readonly unknown[] | null };
export const validateGetDiscoveryIdentityGroupsInput: ContractValidator<GetDiscoveryIdentityGroupsInput>;
