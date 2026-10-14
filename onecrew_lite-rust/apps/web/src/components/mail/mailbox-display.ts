export function mailboxAddressLocalPart(emailAddress: string): string {
  const separator = emailAddress.lastIndexOf("@");
  return separator > 0 ? emailAddress.slice(0, separator) : emailAddress;
}
