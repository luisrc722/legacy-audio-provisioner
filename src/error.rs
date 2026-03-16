use anyhow::Error as AnyhowError;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ProvisioningError {
    #[error("Modo --json no es compatible con {feature}")]
    UnsupportedJsonMode { feature: String },

    #[error("Se requiere --usb-mount junto con --resume")]
    MissingUsbMountForResume,

    #[error("CONCURRENCY_ERROR: {details}")]
    ConcurrencyError { details: String },

    #[error("FILESYSTEM_READ_ONLY: {details}")]
    ReadOnlyFilesystem { details: String },

    #[error("ENOSPC_ERROR: {details}")]
    StorageFull { details: String },

    #[error("HARDWARE_FRAUD_DETECTED: {details}")]
    HardwareFraudDetected { details: String },

    #[error("DRM_PROTECTED: {details}")]
    DrmProtected { details: String },

    #[error("PROVISIONING_FAILED: {details}")]
    ProvisioningFailed { details: String },
}

impl ProvisioningError {
    pub fn code(&self) -> &'static str {
        match self {
            ProvisioningError::UnsupportedJsonMode { .. } => "UNSUPPORTED_JSON_MODE",
            ProvisioningError::MissingUsbMountForResume => "MISSING_USB_MOUNT",
            ProvisioningError::ConcurrencyError { .. } => "CONCURRENCY_ERROR",
            ProvisioningError::ReadOnlyFilesystem { .. } => "FILESYSTEM_READ_ONLY",
            ProvisioningError::StorageFull { .. } => "ENOSPC_ERROR",
            ProvisioningError::HardwareFraudDetected { .. } => "HARDWARE_FRAUD_DETECTED",
            ProvisioningError::DrmProtected { .. } => "DRM_PROTECTED",
            ProvisioningError::ProvisioningFailed { .. } => "PROVISIONING_FAILED",
        }
    }

    pub fn action_required(&self) -> &'static str {
        match self {
            ProvisioningError::UnsupportedJsonMode { .. } => {
                "Use --json solo con --usb-mount/--audio-source o con --resume."
            }
            ProvisioningError::MissingUsbMountForResume => {
                "Agregue --usb-mount <PATH> cuando utilice --resume."
            }
            ProvisioningError::ConcurrencyError { .. } => {
                "Cierre la otra instancia activa o espere a que termine."
            }
            ProvisioningError::ReadOnlyFilesystem { .. } => {
                "Repare la USB con 'sudo fsck.fat -a <dispositivo>' y reintente."
            }
            ProvisioningError::StorageFull { .. } => {
                "Libere espacio en el disco local del host."
            }
            ProvisioningError::HardwareFraudDetected { .. } => {
                "Deseche esta memoria USB (NAND Spoofing detectado)."
            }
            ProvisioningError::DrmProtected { .. } => {
                "Elimine los archivos con DRM de la carpeta de origen."
            }
            ProvisioningError::ProvisioningFailed { .. } => {
                "Revise el mensaje de error y reintente con --resume."
            }
        }
    }

    pub fn from_anyhow(err: AnyhowError) -> Self {
        if let Some(typed) = err.downcast_ref::<ProvisioningError>() {
            return typed.clone();
        }

        for cause in err.chain() {
            if let Some(typed) = cause.downcast_ref::<ProvisioningError>() {
                return typed.clone();
            }
        }

        ProvisioningError::ProvisioningFailed {
            details: err.to_string(),
        }
    }
}
