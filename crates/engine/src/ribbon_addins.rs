//! Add-in (optional module) ribbon tabs for [`RibbonLayout`].
//!
//! Kept beside WordCraft's ribbon core so LawCraft module tabs don't live in that file.

use std::borrow::Cow;

use serde_json::Value;

use super::{CustomGroup, RibbonLayout, TabEntry, clean_name, default_tabs, reserved, same, MAX_TABS};

impl RibbonLayout {
    /// Load hidden add-in tab names from a saved layout value.
    pub(super) fn load_hidden_addin_tabs(&mut self, v: &Value) {
        for name in v.get("hiddenAddinTabs").and_then(Value::as_array).into_iter().flatten().take(MAX_TABS).filter_map(Value::as_str) {
            if let Some(name) = clean_name(name)
                && !self.hidden_addin_tabs.iter().any(|h| same(h, &name))
            {
                self.hidden_addin_tabs.push(name);
            }
        }
    }

    /// The tabs in order (File excluded), customized or not, then the add-ins' tabs.
    pub fn tabs(&self) -> Cow<'_, [TabEntry]> {
        let user = self.user_tabs();
        if self.addin_tabs.is_empty() {
            return user;
        }
        let mut all = user.into_owned();
        for t in &self.addin_tabs {
            if !all.iter().any(|x| same(&x.name, &t.name)) {
                let hidden = self.hidden_addin_tabs.iter().any(|h| same(h, &t.name));
                all.push(TabEntry { hidden, ..t.clone() });
            }
        }
        Cow::Owned(all)
    }

    /// The user's tabs in order (File excluded), customized or not, without the add-ins' tabs.
    pub(super) fn user_tabs(&self) -> Cow<'_, [TabEntry]> {
        if self.tabs.is_empty() { Cow::Owned(default_tabs()) } else { Cow::Borrowed(&self.tabs) }
    }

    /// Whether the ribbon is the default one.
    pub fn ribbon_is_default(&self) -> bool {
        self.tabs.is_empty() && self.hidden_addin_tabs.is_empty()
    }

    /// Show an add-in's tab (`name`, holding `groups` of the add-in's commands) after the user's
    /// tabs, replacing the add-in tab of that name. The user can hide it but not edit it, and it
    /// isn't saved: the add-in calls this each time it is installed. A name a built-in or
    /// contextual tab has, or one of the user's own tabs has, is ignored.
    pub fn set_addin_tab(&mut self, name: &str, groups: Vec<CustomGroup>) {
        let Some(name) = clean_name(name) else { return };
        if reserved(&name) {
            return;
        }
        self.addin_tabs.retain(|t| !same(&t.name, &name));
        self.addin_tabs.push(TabEntry { name, custom: true, hidden: false, groups });
    }

    /// Stop showing an add-in's tab (the add-in was turned off).
    pub fn remove_addin_tab(&mut self, name: &str) {
        self.addin_tabs.retain(|t| !same(&t.name, name));
    }

    /// Whether `name` is an add-in's tab (and not one of the user's).
    fn is_addin_tab(&self, name: &str) -> bool {
        self.addin_tabs.iter().any(|t| same(&t.name, name)) && !self.user_tabs().iter().any(|t| same(&t.name, name))
    }

    /// Show or hide a tab (add-in tabs keep their hidden state in [`RibbonLayout::hidden_addin_tabs`]).
    pub fn set_hidden(&mut self, tab: &str, hidden: bool) -> Result<(), String> {
        if self.is_addin_tab(tab) {
            self.hidden_addin_tabs.retain(|h| !same(h, tab));
            if hidden {
                self.hidden_addin_tabs.push(tab.to_string());
            }
            return Ok(());
        }
        self.edit(|tabs| {
            let i = Self::tab_index(tabs, tab)?;
            if let Some(t) = tabs.get_mut(i) {
                t.hidden = hidden;
            }
            Ok(())
        })
    }

    /// Back to the default ribbon (the Quick Access Toolbar stays).
    pub fn reset_ribbon(&mut self) {
        self.tabs.clear();
        self.hidden_addin_tabs.clear();
    }

    /// Take `other`'s tabs, hidden add-in tabs and toolbar (a loaded or imported layout), keeping
    /// the tabs installed add-ins show: those come from the add-ins, not from a saved layout.
    pub fn replace_layout(&mut self, other: RibbonLayout) {
        let addins = std::mem::take(&mut self.addin_tabs);
        *self = other;
        self.addin_tabs = addins;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::ribbon::TABS;

    /// An add-in's tab shows after the user's tabs, hides like any tab (and that is saved), isn't
    /// saved itself, and goes when the add-in is turned off.
    #[test]
    fn addin_tabs_show_hide_and_are_not_saved() {
        let mut l = RibbonLayout::default();
        let group = CustomGroup { name: "Tools".into(), commands: vec!["format.bold".into()] };
        l.set_addin_tab("Extra", vec![group.clone()]);
        l.set_addin_tab("Home", vec![group.clone()]);
        let tabs = l.tabs();
        assert_eq!(tabs.len(), TABS.len(), "the built-in tabs and Extra, not a second Home");
        let extra = tabs.last().unwrap();
        assert!(extra.name == "Extra" && extra.custom && !extra.hidden && extra.groups == [group.clone()]);
        assert!(l.ribbon_is_default(), "an add-in tab doesn't customize the ribbon");
        assert_eq!(serde_json::to_value(&l).unwrap(), json!({}), "nor is it saved");

        // A custom tab the user adds goes before it; editing never saves it.
        l.new_tab(Some("Mine"), None).unwrap();
        let names: Vec<String> = l.tabs().iter().map(|t| t.name.clone()).collect();
        assert_eq!(names[names.len() - 2..], ["Mine", "Extra"]);
        assert!(!serde_json::to_string(&l).unwrap().contains("Extra"));
        assert!(l.new_group("Extra", None).is_err(), "an add-in tab isn't editable");

        // Hiding it is saved, and holds after a restart and while the add-in is off.
        l.set_hidden("Extra", true).unwrap();
        assert!(l.tabs().last().unwrap().hidden);
        let mut back: RibbonLayout = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
        assert_eq!(back.hidden_addin_tabs, ["Extra"]);
        assert!(back.tabs().iter().all(|t| t.name != "Extra"), "not shown until the add-in is installed");
        back.set_addin_tab("Extra", vec![group.clone()]);
        assert!(back.tabs().last().unwrap().hidden);
        back.set_hidden("Extra", false).unwrap();
        assert!(!back.tabs().last().unwrap().hidden);
        back.remove_addin_tab("Extra");
        assert!(back.tabs().iter().all(|t| t.name != "Extra"));

        // Reset Ribbon shows hidden add-in tabs again.
        l.reset_ribbon();
        assert!(l.ribbon_is_default() && !l.tabs().last().unwrap().hidden);

        // Loading a saved layout keeps the add-in's tab too.
        l.set_hidden("Extra", true).unwrap();
        l.replace_layout(RibbonLayout::default());
        assert!(l.ribbon_is_default());
        let extra = l.tabs().last().unwrap().clone();
        assert!(extra.name == "Extra" && !extra.hidden, "{extra:?}");
    }

    /// Every reset in Customize the Ribbon keeps an installed add-in's tab: the add-in put it
    /// there, and only turning the add-in off takes it away.
    #[test]
    fn reset_keeps_addin_tabs() {
        for reset in [json!(true), json!("ribbon"), json!("qat")] {
            let mut s = crate::Session::new(wordcraft_doc::Document::new());
            let group = CustomGroup { name: "Tools".into(), commands: vec!["format.bold".into()] };
            s.ribbon.set_addin_tab("Extra", vec![group]);
            s.run("tools.customizeRibbon", &json!({"newTab": {"name": "Mine"}, "hide": "Extra", "qatAdd": "file.new"})).unwrap();
            s.run("tools.customizeRibbon", &json!({"reset": reset})).unwrap();
            let names: Vec<String> = s.ribbon.tabs().iter().map(|t| t.name.clone()).collect();
            assert_eq!(names.last().map(String::as_str), Some("Extra"), "{reset}: {names:?}");
            if reset != json!("qat") {
                assert!(!s.ribbon.tabs().last().unwrap().hidden, "{reset}");
                assert!(!names.contains(&"Mine".to_string()), "{reset}");
            }
            if reset != json!("ribbon") {
                assert!(!s.ribbon.qat().contains(&"file.new"), "{reset}");
            }
            s.run("tools.customizeRibbon", &json!({"layout": {}})).unwrap();
            assert!(s.ribbon.tabs().iter().any(|t| t.name == "Extra"), "{reset}: a loaded layout keeps it");
        }
    }
}
