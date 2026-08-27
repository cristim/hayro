use hayro_syntax::object::dict::keys::{BASE_STATE, D, OCGS, OCMD, OCPROPERTIES, OFF, ON, P, TYPE};
use hayro_syntax::object::{Array, Dict, Name, ObjectIdentifier};
use std::collections::{HashMap, HashSet};

pub(crate) struct OcgState {
    inactive_ocgs: HashSet<ObjectIdentifier>,
    visibility_stack: Vec<bool>,
}

impl OcgState {
    pub(crate) fn from_catalog(
        catalog: &Dict<'_>,
        overrides: &HashMap<ObjectIdentifier, bool>,
    ) -> Self {
        let mut inactive = Self::inactive_from_catalog(catalog);

        for (ocg, visible) in overrides {
            if *visible {
                inactive.remove(ocg);
            } else {
                inactive.insert(*ocg);
            }
        }

        Self {
            inactive_ocgs: inactive,
            visibility_stack: Vec::new(),
        }
    }

    /// The optional content groups turned off by the document's default
    /// configuration.
    fn inactive_from_catalog(catalog: &Dict<'_>) -> HashSet<ObjectIdentifier> {
        let Some(oc_properties) = catalog.get::<Dict<'_>>(OCPROPERTIES) else {
            return HashSet::default();
        };

        let Some(config) = oc_properties.get::<Dict<'_>>(D) else {
            return HashSet::default();
        };

        let mut inactive = HashSet::new();

        let base_state = config
            .get::<Name<'_>>(BASE_STATE)
            .and_then(|b| BaseState::from_name(b.as_ref()));

        if base_state.unwrap_or(BaseState::On) == BaseState::Off
            && let Some(ocgs) = oc_properties.get::<Array<'_>>(OCGS)
        {
            for item in ocgs.raw_iter() {
                if let Some(ref_) = item.as_obj_ref() {
                    let id: ObjectIdentifier = ref_.into();
                    inactive.insert(id);
                }
            }
        }

        let mut read_ocg_array = |key, insert_active: bool| {
            if let Some(arr) = config.get::<Array<'_>>(key) {
                for item in arr.raw_iter() {
                    if let Some(ref_) = item.as_obj_ref() {
                        let id: ObjectIdentifier = ref_.into();
                        if insert_active {
                            inactive.remove(&id);
                        } else {
                            inactive.insert(id);
                        }
                    }
                }
            }
        };

        read_ocg_array(ON, true);
        read_ocg_array(OFF, false);

        inactive
    }

    pub(crate) fn begin_single_oc(&mut self, ocg_id: ObjectIdentifier) {
        let is_active = !self.inactive_ocgs.contains(&ocg_id);
        let visible = self.is_visible() && is_active;
        self.visibility_stack.push(visible);
    }

    pub(crate) fn begin_ocmd(&mut self, ocmd: &Dict<'_>) {
        let policy = ocmd
            .get::<Name<'_>>(P)
            .and_then(|n| OcmdPolicy::from_name(n.as_ref()))
            .unwrap_or(OcmdPolicy::AnyOn);

        let mut ocg_ids: Vec<ObjectIdentifier> = Vec::new();

        if let Some(arr) = ocmd.get::<Array<'_>>(OCGS) {
            for item in arr.raw_iter() {
                if let Some(ref_) = item.as_obj_ref() {
                    ocg_ids.push(ref_.into());
                }
            }
        } else if let Some(ref_) = ocmd.get_ref(OCGS) {
            ocg_ids.push(ref_.into());
        }

        let is_active = if ocg_ids.is_empty() {
            true
        } else {
            match policy {
                OcmdPolicy::AllOn => ocg_ids.iter().all(|id| !self.inactive_ocgs.contains(id)),
                OcmdPolicy::AnyOn => ocg_ids.iter().any(|id| !self.inactive_ocgs.contains(id)),
                OcmdPolicy::AnyOff => ocg_ids.iter().any(|id| self.inactive_ocgs.contains(id)),
                OcmdPolicy::AllOff => ocg_ids.iter().all(|id| self.inactive_ocgs.contains(id)),
            }
        };

        let visible = self.is_visible() && is_active;
        self.visibility_stack.push(visible);
    }

    pub(crate) fn begin_ocg(&mut self, props: &Dict<'_>, ref_id: ObjectIdentifier) {
        match props.get::<Name<'_>>(TYPE).as_deref() {
            Some(OCMD) => self.begin_ocmd(props),
            _ => self.begin_single_oc(ref_id),
        }
    }

    pub(crate) fn begin_marked_content(&mut self) {
        let visible = self.is_visible();
        self.visibility_stack.push(visible);
    }

    pub(crate) fn end_marked_content(&mut self) {
        self.visibility_stack.pop();
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visibility_stack.last().copied().unwrap_or(true)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum BaseState {
    On,
    Off,
    Unchanged,
}

impl BaseState {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"ON" => Some(Self::On),
            b"OFF" => Some(Self::Off),
            b"Unchanged" => Some(Self::Unchanged),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum OcmdPolicy {
    AllOn,
    AnyOn,
    AnyOff,
    AllOff,
}

impl OcmdPolicy {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"AllOn" => Some(Self::AllOn),
            b"AnyOn" => Some(Self::AnyOn),
            b"AnyOff" => Some(Self::AnyOff),
            b"AllOff" => Some(Self::AllOff),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hayro_syntax::reader::{Reader, ReaderContext, ReaderExt};

    /// Two optional content groups, the second one turned off by the default
    /// configuration.
    const CATALOG: &[u8] = b"<< /OCProperties << /OCGs [4 0 R 5 0 R] /D << /OFF [5 0 R] >> >> >>";

    fn catalog(data: &[u8]) -> Dict<'_> {
        Reader::new(data)
            .read_with_context::<Dict<'_>>(&ReaderContext::dummy())
            .unwrap()
    }

    fn first() -> ObjectIdentifier {
        ObjectIdentifier::new(4, 0)
    }

    fn second() -> ObjectIdentifier {
        ObjectIdentifier::new(5, 0)
    }

    fn is_visible(
        catalog: &Dict<'_>,
        overrides: &HashMap<ObjectIdentifier, bool>,
        ocg: ObjectIdentifier,
    ) -> bool {
        let mut state = OcgState::from_catalog(catalog, overrides);
        state.begin_single_oc(ocg);

        state.is_visible()
    }

    #[test]
    fn no_overrides() {
        let catalog = catalog(CATALOG);
        let overrides = HashMap::new();

        assert!(is_visible(&catalog, &overrides, first()));
        assert!(!is_visible(&catalog, &overrides, second()));
    }

    #[test]
    fn overrides_replace_the_default_configuration() {
        let catalog = catalog(CATALOG);
        let overrides = HashMap::from([(first(), false), (second(), true)]);

        assert!(!is_visible(&catalog, &overrides, first()));
        assert!(is_visible(&catalog, &overrides, second()));
    }

    #[test]
    fn overrides_without_optional_content_properties() {
        let catalog = catalog(b"<< /Type /Catalog >>");
        let overrides = HashMap::from([(first(), false)]);

        assert!(!is_visible(&catalog, &overrides, first()));
        assert!(is_visible(&catalog, &overrides, second()));
    }

    #[test]
    fn overrides_apply_to_ocmd_policies() {
        let root = catalog(CATALOG);
        let is_visible = |overrides: &HashMap<ObjectIdentifier, bool>, policy: &[u8]| {
            let mut data = b"<< /Type /OCMD /OCGs [4 0 R 5 0 R] /P /".to_vec();
            data.extend_from_slice(policy);
            data.extend_from_slice(b" >>");

            let mut state = OcgState::from_catalog(&root, overrides);
            state.begin_ocmd(&catalog(&data));

            state.is_visible()
        };

        // The default configuration has the first group on and the second off.
        let none = HashMap::new();
        assert!(is_visible(&none, b"AnyOn"));
        assert!(!is_visible(&none, b"AllOn"));

        // Turning the second one on satisfies AllOn, and leaves nothing off.
        let second_on = HashMap::from([(second(), true)]);
        assert!(is_visible(&second_on, b"AllOn"));
        assert!(!is_visible(&second_on, b"AnyOff"));

        // Turning both off leaves nothing on.
        let both_off = HashMap::from([(first(), false), (second(), false)]);
        assert!(!is_visible(&both_off, b"AnyOn"));
        assert!(is_visible(&both_off, b"AllOff"));
    }
}
