use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::value::Value;

#[derive(Clone)]
pub struct Environment {
    values: RefCell<HashMap<String, (Value, Option<String>)>>,
    exports: RefCell<HashSet<String>>,
    parent: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new() -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self { values: RefCell::new(HashMap::new()), exports: RefCell::new(HashSet::new()), parent: None }))
    }

    pub fn with_parent(parent: Rc<RefCell<Self>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self { values: RefCell::new(HashMap::new()), exports: RefCell::new(HashSet::new()), parent: Some(parent) }))
    }

    pub fn add_export(&self, name: &str) {
        self.exports.borrow_mut().insert(name.to_string());
    }

    pub fn exported_names(&self) -> HashSet<String> {
        self.exports.borrow().clone()
    }

    pub fn define(&self, name: &str, value: Value, type_ann: Option<String>) {
        self.values.borrow_mut().insert(name.to_string(), (value, type_ann));
    }

    pub fn define_untyped(&self, name: &str, value: Value) {
        self.define(name, value, None);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some((v, _)) = self.values.borrow().get(name) {
            return Some(v.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().get(name);
        }
        None
    }

    pub fn get_type(&self, name: &str) -> Option<Option<String>> {
        if let Some((_, t)) = self.values.borrow().get(name) {
            return Some(t.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().get_type(name);
        }
        None
    }

    pub fn set(&self, name: &str, value: Value) -> Result<(), String> {
        let mut values = self.values.borrow_mut();
        if let Some((_, type_ann)) = values.get(name) {
            let type_ann = type_ann.clone();
            values.insert(name.to_string(), (value, type_ann));
            return Ok(());
        }
        drop(values);
        if let Some(parent) = &self.parent {
            return parent.borrow().set(name, value);
        }
        Err(format!("Undefined variable '{}'", name))
    }

    pub fn entries(&self) -> Vec<(String, Value, Option<String>)> {
        self.values
            .borrow()
            .iter()
            .map(|(name, (value, type_ann))| (name.clone(), value.clone(), type_ann.clone()))
            .collect()
    }
}
