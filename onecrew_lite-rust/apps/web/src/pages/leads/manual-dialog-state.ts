export function shouldApplyUserOpenChange(pending: boolean, nextOpen: boolean): boolean {
  return nextOpen || !pending;
}

export function openAfterSuccessfulSubmit(): false {
  return false;
}

export type SubmissionLock = { current: boolean };

export type DialogMutationRun<T> =
  | { started: false }
  | { started: true; value: T };

export async function runDialogMutationOnce<T>({
  lock,
  onPendingChange,
  mutation,
  afterSuccess,
}: {
  lock: SubmissionLock;
  onPendingChange: (pending: boolean) => void;
  mutation: () => Promise<T>;
  afterSuccess: () => Promise<void>;
}): Promise<DialogMutationRun<T>> {
  if (lock.current) return { started: false };
  lock.current = true;
  onPendingChange(true);
  try {
    const value = await mutation();
    await afterSuccess();
    return { started: true, value };
  } finally {
    lock.current = false;
    onPendingChange(false);
  }
}
