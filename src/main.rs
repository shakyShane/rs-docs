use rustdoc_types::{
    Crate, GenericArg, GenericArgs, Id, Impl, Item, ItemEnum, ItemKind, ItemSummary, Path,
    StructKind, Type,
};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug)]
enum Known {
    SerdeSerialize,
    StdString,
    Option,
}

#[derive(Debug)]
enum Value {
    ZodString,
    ZodNumber,
    ZodIdent { ident: String },
}

#[derive(Debug)]
struct ZodObject {
    name: String,
    fields: Vec<Field>,
}

#[derive(Debug)]
enum Outputs {
    ZodObject(ZodObject),
}

#[derive(Debug)]
struct Output {
    types: Vec<Outputs>,
}

#[derive(Debug)]
enum Field {
    Required { field: StructField },
    Optional { field: StructField },
}

impl Field {
    pub fn required(field: StructField) -> Self {
        Self::Required { field }
    }
    pub fn optional(field: StructField) -> Self {
        Self::Optional { field }
    }
}

#[derive(Debug)]
struct StructField {
    name: String,
    value: Value,
}

impl StructField {
    pub fn new(name: &str, value: Value) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Debug)]
struct OptionalField {
    name: String,
    value: Value,
}

fn main() {
    let mut c = Command::new("cargo");
    c.args([
        "+nightly",
        "rustdoc",
        "-p",
        "docs",
        "--lib",
        "--",
        "-Zunstable-options",
        "-wjson",
    ]);
    match c.spawn() {
        Ok(_) => println!("generated"),
        Err(e) => {
            eprintln!("{:?}", e);
            panic!("error")
        }
    }
    let s = include_str!("/Users/shaneosbourne/CLionProjects/docs/target/doc/docs_lib.json");
    let t: Crate = serde_json::from_str(s).unwrap();
    let mut known_mapping: HashMap<Id, Known> = HashMap::new();
    let ser_matcher = vec!["serde", "ser", "Serialize"];
    let mut ser_id: Option<Id> = None;
    let string_matcher = vec!["alloc", "string", "String"];
    let option_matcher = vec!["core", "option", "Option"];
    for (id, item_summary) in &t.paths {
        if item_summary.path == ser_matcher {
            println!("{:?}", item_summary.path);
            ser_id = Some(id.clone());
            known_mapping.insert(id.clone(), Known::SerdeSerialize);
        }
        if item_summary.path == string_matcher {
            known_mapping.insert(id.clone(), Known::StdString);
        }
        if item_summary.path == option_matcher {
            known_mapping.insert(id.clone(), Known::Option);
        }
    }
    let mut output = Output { types: vec![] };
    if let Some(ser_id) = ser_id {
        // println!("{:?}", id);
        // dbg!(t.index.get(&id));
        // now find all impl items that use this ID
        for (_, item) in &t.index {
            match &item.inner {
                ItemEnum::Impl(Impl {
                    trait_: Some(ref trait_path),
                    for_,
                    ..
                }) => {
                    if trait_path.id == ser_id {
                        match for_ {
                            Type::ResolvedPath(rp) => {
                                let matching_item =
                                    t.index.get(&rp.id).expect("guarded index lookup");
                                output
                                    .types
                                    .extend(gen_for(&matching_item, &t, &known_mapping));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    print(&output);
}

fn print(output: &Output) {
    dbg!(output);
}

fn gen_for(item: &Item, data: &Crate, known: &HashMap<Id, Known>) -> Vec<Outputs> {
    let mut outputs: Vec<Outputs> = vec![];
    match &item.inner {
        ItemEnum::Module(_) => {}
        ItemEnum::ExternCrate { .. } => {}
        ItemEnum::Import(_) => {}
        ItemEnum::Union(_) => {}
        ItemEnum::Struct(struct_) => {
            let name = item.name.as_ref().expect("oops").to_string();
            let mut zod_object = ZodObject {
                fields: vec![],
                name,
            };
            match &struct_.kind {
                StructKind::Unit => {}
                StructKind::Tuple(_) => {}
                StructKind::Plain { fields, .. } => {
                    for id in fields {
                        let item = data.index.get(&id).expect("guarded");

                        match &item.inner {
                            ItemEnum::StructField(Type::Primitive(prim)) => {
                                let as_value = rust_prim_to_zod(prim);
                                zod_object.fields.push(Field::Required {
                                    field: StructField::new(&field_name_for_item(&item), as_value),
                                })
                            }
                            ItemEnum::StructField(Type::ResolvedPath(ref rp)) => {
                                if let Some(field) = handle_struct_field(known, &data, &item, &rp) {
                                    zod_object.fields.push(field);
                                }
                            }
                            _ => todo!("on a struct, can we get here?"),
                        }
                    }
                }
            }
            if !zod_object.fields.is_empty() {
                outputs.push(Outputs::ZodObject(zod_object));
            } else {
                eprintln!("missing fields, so not adding {:?}", zod_object);
            }
        }
        ItemEnum::StructField(_) => {}
        ItemEnum::Enum(_) => {}
        ItemEnum::Variant(_) => {}
        ItemEnum::Function(_) => {}
        ItemEnum::Trait(_) => {}
        ItemEnum::TraitAlias(_) => {}
        ItemEnum::Impl(_) => {}
        ItemEnum::Typedef(_) => {}
        ItemEnum::OpaqueTy(_) => {}
        ItemEnum::Constant(_) => {}
        ItemEnum::Static(_) => {}
        ItemEnum::ForeignType => {}
        ItemEnum::Macro(_) => {}
        ItemEnum::ProcMacro(_) => {}
        ItemEnum::Primitive(_) => {}
        ItemEnum::AssocConst { .. } => {}
        ItemEnum::AssocType { .. } => {}
    }
    outputs
}

fn handle_struct_field(
    known: &HashMap<Id, Known>,
    data: &Crate,
    item: &Item,
    rp: &Path,
) -> Option<Field> {
    let field_name = field_name_for_item(&item);
    known
        .get(&rp.id)
        .and_then(|known_item| {
            dbg!("rp", &rp.name);
            match known_item {
                // if we are here, we are trying to match against a path we already knew about, like std::string, or option etc// if we are here, we are trying to match against a path we already knew about, like std::string, or option etc
                Known::SerdeSerialize => None,
                Known::StdString => handle_string(&field_name),
                Known::Option => handle_option(&item, rp),
            }
        })
        .or_else(|| {
            // but here, we have a 'resolved_path', although we don't know what it is
            // it's likely an internal module etc
            data.paths
                .get(&rp.id)
                .map(|item_summary| handle_ident(&field_name, item_summary))
        })
}

fn handle_string(field_name: &String) -> Option<Field> {
    Some(Field::required(StructField::new(
        &field_name,
        Value::ZodString,
    )))
}

// from the
fn handle_ident(field_name: &String, item_summary: &ItemSummary) -> Field {
    match item_summary.kind {
        ItemKind::Struct => Field::required(StructField::new(
            &field_name,
            Value::ZodIdent {
                ident: item_summary
                    .path
                    .last()
                    .expect("must have a last path sement")
                    .to_string(),
            },
        )),
        _ => todo!("more ident support"),
    }
}

fn handle_option(item: &Item, rp: &Path) -> Option<Field> {
    match rp.args.as_ref().expect("e").as_ref() {
        GenericArgs::AngleBracketed { args, .. } => {
            for x in args {
                match x {
                    GenericArg::Lifetime(_) => {}
                    GenericArg::Type(Type::Primitive(prim)) => {
                        let rt = rust_prim_to_zod(prim);
                        return Some(Field::optional(StructField {
                            name: field_name_for_item(&item),
                            value: rt,
                        }));
                    }
                    GenericArg::Type(other) => {
                        todo!("other types on GenericArg");
                    }
                    GenericArg::Const(_) => {}
                    GenericArg::Infer => {}
                }
            }
        }
        GenericArgs::Parenthesized { .. } => {}
    }
    None
}

fn rust_prim_to_zod(prim: &str) -> Value {
    match prim {
        "u8" => Value::ZodNumber,
        _ => todo!("match more primitives"),
    }
}

fn field_name_for_item(item: &Item) -> String {
    item.name
        .as_ref()
        .expect("name must exist on struct field")
        .clone()
}
