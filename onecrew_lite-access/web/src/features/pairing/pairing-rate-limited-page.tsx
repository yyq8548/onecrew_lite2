import { ArrowLeft, Clock3 } from "lucide-react";
import { useNavigate } from "react-router";

import { BrandMark } from "@/components/brand-mark";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export function PairingRateLimitedPage() {
  const navigate = useNavigate();
  return (
    <main className="min-h-svh bg-muted/35">
      <header className="border-b border-border bg-background">
        <div className="mx-auto flex max-w-3xl items-center px-4 py-4 sm:px-6">
          <BrandMark />
        </div>
      </header>
      <section className="mx-auto w-full max-w-3xl px-4 py-10 sm:px-6 sm:py-16">
        <Card className="mx-auto max-w-xl">
          <CardHeader>
            <CardTitle asChild><h1>操作过于频繁</h1></CardTitle>
          </CardHeader>
          <CardContent className="space-y-5">
            <Alert>
              <Clock3 aria-hidden="true" />
              <AlertTitle>请稍后再试</AlertTitle>
              <AlertDescription>
                当前账号的绑定确认已达到短时上限。请等待几分钟后，从 OneCrew Desktop 重新发起绑定。
              </AlertDescription>
            </Alert>
            <Button className="w-full sm:w-auto" onClick={() => navigate("/account", { replace: true })}>
              <ArrowLeft aria-hidden="true" />返回账号中心
            </Button>
          </CardContent>
        </Card>
      </section>
    </main>
  );
}
