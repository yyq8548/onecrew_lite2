import { useCallback, useEffect, useState } from "react";

export const OTP_RESEND_COOLDOWN_SECONDS = 60;

export function useOtpCooldown() {
  const [cooldownUntil, setCooldownUntil] = useState<number | null>(null);
  const [cooldownSeconds, setCooldownSeconds] = useState(0);

  useEffect(() => {
    if (cooldownUntil === null) {
      setCooldownSeconds(0);
      return;
    }
    const deadline = cooldownUntil;

    function update() {
      const remaining = Math.max(
        0,
        Math.ceil((deadline - Date.now()) / 1_000),
      );
      setCooldownSeconds(remaining);
      if (remaining === 0) setCooldownUntil(null);
    }

    update();
    const timer = window.setInterval(update, 1_000);
    return () => window.clearInterval(timer);
  }, [cooldownUntil]);

  const startCooldown = useCallback(() => {
    setCooldownUntil(Date.now() + OTP_RESEND_COOLDOWN_SECONDS * 1_000);
  }, []);

  return { cooldownSeconds, startCooldown };
}
