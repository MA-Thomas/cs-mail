//! Local custody adapter. Host-provisioned secrets never enter the message database.
//! This is an operational trust boundary, not an HSM or protection against the host operator.
use cs_mail_application::consent::{AuthorizedDecryption, LocalKeyCustody, ReleasedCopy};
use cs_mail_consent::ConsentError;
use cs_mail_content::{EndpointPublicKey, EndpointSecretKey, decrypt, encrypt};
use cs_mail_correspondence::Document;
use cs_mail_primitives::ContentKeyRef;
use zeroize::Zeroizing;

pub struct LocalCustodian {
    secret: EndpointSecretKey,
    public: EndpointPublicKey,
}
impl LocalCustodian {
    /// # Errors
    /// The host restores this same secret across restarts using its protected secret store.
    pub fn from_secret_bytes(
        reference: ContentKeyRef,
        secret: &[u8; 32],
    ) -> Result<Self, ConsentError> {
        let (secret, public) = EndpointSecretKey::from_secret_bytes(reference, secret)
            .map_err(|_| ConsentError::Invalid)?;
        Ok(Self { secret, public })
    }
    pub const fn public_key(&self) -> EndpointPublicKey {
        self.public
    }
}
impl LocalKeyCustody for LocalCustodian {
    fn public_key(&self) -> EndpointPublicKey {
        self.public
    }
    fn release(
        &self,
        authorization: AuthorizedDecryption<'_>,
    ) -> Result<ReleasedCopy, ConsentError> {
        let source = authorization.source();
        let plaintext = Zeroizing::new(
            decrypt(&self.secret, source.sender, &source.ciphertext)
                .map_err(|_| ConsentError::Unavailable)?,
        );
        let document: Document =
            serde_json::from_slice(&plaintext).map_err(|_| ConsentError::Invalid)?;
        if document
            .manifest(
                authorization.manifest().message(),
                authorization.manifest().conversation(),
            )
            .map_err(|_| ConsentError::Invalid)?
            != *authorization.manifest()
        {
            return Err(ConsentError::Invalid);
        }
        let ciphertext = encrypt(
            &self.secret,
            authorization.destination(),
            source.ciphertext.binding.clone(),
            &plaintext,
            authorization.at(),
            authorization.expires_at(),
        )
        .map_err(|_| ConsentError::Invalid)?;
        Ok(ReleasedCopy {
            manifest: authorization.manifest().clone(),
            ciphertext,
            custodian: self.public,
        })
    }
}
