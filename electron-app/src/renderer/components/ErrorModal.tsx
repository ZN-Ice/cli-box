import React from "react";

interface ErrorModalProps {
  title: string;
  message: string;
  onRetry?: () => void;
  onClose: () => void;
}

export function ErrorModal({ title, message, onRetry, onClose }: ErrorModalProps) {
  return (
    <div className="error-modal-overlay" onClick={onClose}>
      <div className="error-modal" onClick={(e) => e.stopPropagation()}>
        <div className="error-modal-header">
          <h2 className="error-modal-title">{title}</h2>
        </div>
        <div className="error-modal-body">
          <pre className="error-modal-message">{message}</pre>
        </div>
        <div className="error-modal-actions">
          {onRetry && (
            <button className="error-modal-button primary" onClick={onRetry}>
              Retry
            </button>
          )}
          <button className="error-modal-button" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}