"use client";

import { Pencil, Check, X } from "lucide-react";
import { useState, useRef, useEffect } from "react";

import { Button } from "@/components/ui/button";

interface EditableTitleProps {
  title: string;
  onSave: (newTitle: string) => Promise<void>;
  className?: string;
  maxLength?: number;
  renderTitle?: (title: string) => React.ReactNode;
}

export function EditableTitle({
  title,
  onSave,
  className = "",
  maxLength = 500,
  renderTitle,
}: EditableTitleProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editedTitle, setEditedTitle] = useState(title);
  const [isSaving, setIsSaving] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setEditedTitle(title);
  }, [title]);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  const handleStartEdit = () => {
    setIsEditing(true);
    setEditedTitle(title);
  };

  const handleCancel = () => {
    setIsEditing(false);
    setEditedTitle(title);
  };

  const handleSave = async () => {
    const trimmedTitle = editedTitle.trim();
    if (!trimmedTitle) {
      return;
    }
    if (trimmedTitle === title) {
      setIsEditing(false);
      return;
    }

    setIsSaving(true);
    try {
      await onSave(trimmedTitle);
      setIsEditing(false);
    } catch (error) {
      console.error("Failed to save title:", error);
      // Keep editing mode on error so user can retry
    } finally {
      setIsSaving(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Enter") {
      e.preventDefault();
      void handleSave();
    } else if (e.key === "Escape") {
      e.preventDefault();
      handleCancel();
    }
  };

  if (isEditing) {
    return (
      <div className={`flex items-center gap-2 ${className}`}>
        <input
          ref={inputRef}
          type="text"
          value={editedTitle}
          onChange={(e) => setEditedTitle(e.target.value.slice(0, maxLength))}
          onKeyDown={handleKeyDown}
          onClick={(e) => e.stopPropagation()}
          className="flex-1 px-2 py-1 text-sm font-semibold bg-background border border-primary rounded focus:outline-none focus:ring-2 focus:ring-primary min-w-[200px]"
          disabled={isSaving}
          maxLength={maxLength}
        />
        <Button
          variant="ghost"
          size="icon"
          onClick={(e) => {
            e.stopPropagation();
            void handleSave();
          }}
          disabled={isSaving || !editedTitle.trim()}
          className="h-8 w-8"
        >
          <Check className="h-4 w-4 text-green-500" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={(e) => {
            e.stopPropagation();
            handleCancel();
          }}
          disabled={isSaving}
          className="h-8 w-8"
        >
          <X className="h-4 w-4 text-muted-foreground" />
        </Button>
      </div>
    );
  }

  return (
    <div className={`flex items-center gap-2 group ${className}`}>
      {renderTitle ? (
        renderTitle(title)
      ) : (
        <span className="text-lg font-semibold">{title}</span>
      )}
      <Button
        variant="ghost"
        size="icon"
        onClick={(e) => {
          e.stopPropagation();
          handleStartEdit();
        }}
        className="h-6 w-6 opacity-0 group-hover:opacity-100 transition-opacity"
        aria-label="Edit title"
      >
        <Pencil className="h-3 w-3" />
      </Button>
    </div>
  );
}
