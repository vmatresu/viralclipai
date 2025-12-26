"use client";

import { CreditCard, Star, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import type { PaymentMethod } from "@/types/billing";

interface PaymentMethodListProps {
  paymentMethods: PaymentMethod[];
  onSetDefault: (paymentMethodId: string) => void;
  onDelete: (paymentMethodId: string) => void;
  isLoading?: boolean;
  loadingId?: string;
}

function getCardIcon(brand: string) {
  // Simple card brand display - could be replaced with actual brand icons
  const brands: Record<string, string> = {
    visa: "Visa",
    mastercard: "Mastercard",
    amex: "American Express",
    discover: "Discover",
  };
  return brands[brand.toLowerCase()] ?? brand;
}

export function PaymentMethodList({
  paymentMethods,
  onSetDefault,
  onDelete,
  isLoading,
  loadingId,
}: PaymentMethodListProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Payment Methods</CardTitle>
        <CardDescription>Manage your saved payment methods</CardDescription>
      </CardHeader>
      <CardContent>
        {paymentMethods.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            No payment methods saved
          </p>
        ) : (
          <div className="space-y-3">
            {paymentMethods.map((method) => (
              <div
                key={method.id}
                className="flex items-center justify-between rounded-lg border p-4"
              >
                <div className="flex items-center gap-3">
                  <CreditCard className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-medium">
                        {getCardIcon(method.brand)}
                      </span>
                      <span className="text-muted-foreground">
                        **** {method.last4}
                      </span>
                      {method.is_default && (
                        <Badge variant="secondary" className="text-xs">
                          Default
                        </Badge>
                      )}
                    </div>
                    <p className="text-sm text-muted-foreground">
                      Expires {method.exp_month.toString().padStart(2, "0")}/
                      {method.exp_year}
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  {!method.is_default && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => onSetDefault(method.id)}
                      disabled={isLoading && loadingId === method.id}
                    >
                      <Star className="h-4 w-4" />
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => onDelete(method.id)}
                    disabled={isLoading && loadingId === method.id}
                  >
                    <Trash2 className="h-4 w-4 text-destructive" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
