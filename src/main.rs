use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use json::JsonValue;
use lazy_static::lazy_static;

fn main() {
    let recipe_data: JsonValue = json::parse(
        ureq::get("https://raw.githubusercontent.com/Seggan/SFCalc-Online/master/src/items.json")
            .call()
            .unwrap()
            .into_string()
            .unwrap()
            .as_str(),
    )
    .unwrap();
    let mut items = HashMap::new();
    let mut item_json = HashMap::new();
    for json in recipe_data.members() {
        let item = SFItem::new(json.clone());
        item_json.insert(item.id.clone(), json.clone());
        items.insert(item.id.clone(), item);
    }
    for item in items.values() {
        item.load(item_json.get(item.id.as_str()).unwrap(), &items);
    }
    /*let item = items.get("ENHANCED_FURNACE_2").unwrap();
    println!("item: {item:#?}");*/
    let mut crafter = CraftingCalculator::new();
    for arg in std::env::args().skip(1) {
        let already_have = arg.starts_with('#');
        let processed_arg = if already_have {
            arg.split_at(1).1
        } else {
            arg.as_str()
        };
        let (id, num) = processed_arg
            .split_once(":")
            .expect(format!("malformed argument {}", arg).as_str());
        let num: u32 = num
            .parse()
            .expect(format!("item count not a number for {}", arg).as_str());
        if already_have {
            crafter.add_extra(
                items
                    .get(id)
                    .expect(format!("item id not found for {}", arg).as_str()),
                num,
            )
        } else {
            crafter.craft(
                items
                    .get(id)
                    .expect(format!("item id not found for {}", arg).as_str()),
                num,
            );
        }
    }
    //crafter.craft(items.get("ENHANCED_FURNACE_2").unwrap(), 1);
    crafter.print(&items);
}
#[derive(Debug)]
struct SFItem {
    name: String,
    id: String,
    recipe: RefCell<Option<(String, Vec<(Item, u32)>, u32)>>,
}
impl SFItem {
    pub fn new(json: JsonValue) -> Rc<SFItem> {
        Rc::new(SFItem {
            name: json["name"].as_str().unwrap().to_string(),
            id: json["id"].as_str().unwrap().to_string(),
            recipe: RefCell::new(None),
        })
    }
    pub fn load(&self, recipe_json: &JsonValue, items: &HashMap<String, Rc<SFItem>>) {
        let mut recipes = Vec::new();
        for recipe in recipe_json["recipe"].members() {
            let item_name = recipe["value"].as_str().unwrap();
            let item = match items.get(item_name) {
                Some(item) => Item::SF(item.clone()),
                None => Item::Vanilla(item_name.to_string()),
            };
            recipes.push((item, recipe["amount"].as_u32().unwrap()));
        }
        *self.recipe.borrow_mut() = Some((
            recipe_json["recipeType"].as_str().unwrap().to_string(),
            recipes,
            recipe_json["result"].as_u32().unwrap(),
        ));
    }
    pub fn get_recipe_depth(&self) -> u32 {
        //todo:cache
        self.recipe
            .borrow()
            .as_ref()
            .unwrap()
            .1
            .iter()
            .map(|item| item.0.get_recipe_depth())
            .max()
            .unwrap_or(0)
            + 1
    }
}
#[derive(Debug)]
enum Item {
    SF(Rc<SFItem>),
    Vanilla(String),
}
impl Item {
    pub fn get_recipe_depth(&self) -> u32 {
        match self {
            Self::SF(item) => item.get_recipe_depth(),
            Self::Vanilla(_) => 1,
        }
    }
    pub fn get_id(&self) -> &str {
        match self {
            Self::SF(item) => item.id.as_str(),
            Self::Vanilla(id) => id.as_str(),
        }
    }
    pub fn get_name(&self) -> &str {
        match self {
            Self::SF(item) => item.name.as_str(),
            Self::Vanilla(id) => id.as_str(),
        }
    }
}
struct CraftingCalculator {
    base_materials: HashMap<String, u32>,
    completed_crafts: HashMap<String, u32>,
    extra_crafts: HashMap<String, u32>,
}
lazy_static! {
    static ref SF_RAW: HashSet<String> = {
        let mut set: HashSet<String> = [
            "SULFATE",
            "BASIC_CIRCUIT_BOARD",
            "IRON_DUST",
            "GOLD_DUST",
            "COPPER_DUST",
            "TIN_DUST",
            "SILVER_DUST",
            "ALUMINUM_DUST",
            "LEAD_DUST",
            "ZINC_DUST",
            "MAGNESIUM_DUST",
            "PULVERIZED_ORE",
        ]
        .iter()
        .map(|str| str.to_string())
        .collect();
        for i in 0..=10 {
            set.insert(format!("GOLD_{}K", (i + 2) * 2));
        }
        set
    };
}
impl CraftingCalculator {
    fn new() -> Self {
        CraftingCalculator {
            base_materials: HashMap::new(),
            completed_crafts: HashMap::new(),
            extra_crafts: HashMap::new(),
        }
    }
    fn craft(&mut self, item: &Rc<SFItem>, mut count: u32) {
        let recipe = item.recipe.borrow();
        let recipe = recipe.as_ref().unwrap();
        {
            let extra = self.extra_crafts(&item.id);
            let already_crafted = count.min(*extra);
            *extra -= already_crafted;
            count -= already_crafted;
        }
        let craft_count = (count as f32 / recipe.2 as f32).ceil() as u32;
        {
            let extra = self.extra_crafts(&item.id);
            *extra += (craft_count * recipe.2) - count;
        }
        {
            *self.completed_crafts(&item.id) += craft_count;
        }
        for ingredient in &recipe.1 {
            let ingredient_id = ingredient.0.get_id();
            let ingredient_count = craft_count * ingredient.1;
            match (SF_RAW.contains(ingredient_id), &ingredient.0) {
                (false, Item::SF(item)) => self.craft(item, ingredient_count),
                (false, Item::Vanilla(material)) => {
                    self.add_base_material(material.clone(), ingredient_count)
                }
                (true, _) => self.add_base_material(ingredient_id.to_string(), ingredient_count),
            }
        }
    }
    pub fn add_extra(&mut self, item: &Rc<SFItem>, count: u32) {
        *self.extra_crafts(&item.id) += count;
    }
    fn completed_crafts(&mut self, id: &str) -> &mut u32 {
        if !self.completed_crafts.contains_key(id) {
            self.completed_crafts.insert(id.to_string(), 0);
        }
        self.completed_crafts.get_mut(id).unwrap()
    }
    fn extra_crafts(&mut self, id: &str) -> &mut u32 {
        if !self.extra_crafts.contains_key(id) {
            self.extra_crafts.insert(id.to_string(), 0);
        }
        self.extra_crafts.get_mut(id).unwrap()
    }
    fn add_base_material(&mut self, material: String, count: u32) {
        let new_count = self.base_materials.get(&material).unwrap_or(&0) + count;
        self.base_materials.insert(material, new_count);
    }
    fn print(&self, items: &HashMap<String, Rc<SFItem>>) {
        println!("BASE MATERIALS: ");
        for base_material in &self.base_materials {
            println!(
                "\t{}:{}",
                remove_formatting(base_material.0),
                base_material.1
            );
        }
        let mut recipes: Vec<(&Rc<SFItem>, &u32)> = self
            .completed_crafts
            .iter()
            .map(|id| (items.get(id.0).unwrap(), id.1))
            .collect();
        recipes.sort_by(|a, b| {
            let a = a.0.get_recipe_depth();
            let b = b.0.get_recipe_depth();
            a.cmp(&b)
        });
        for recipe_item in recipes {
            let recipe = recipe_item.0.recipe.borrow();
            let recipe = recipe.as_ref().unwrap();

            println!(
                "{}({}):{}",
                remove_formatting(recipe_item.0.name.as_str()),
                recipe_item.1,
                recipe.0
            );
            for ingredient in &recipe.1 {
                println!(
                    "\t{}x{}({})",
                    ingredient.1,
                    remove_formatting(ingredient.0.get_name()),
                    ingredient.1 * recipe_item.1
                );
            }
        }
    }
}
fn remove_formatting(text: &str) -> String {
    let mut new_text = String::with_capacity(text.len());
    let mut skip = false;
    for ch in text.chars() {
        if skip {
            skip = false;
            continue;
        }
        if ch == '§' {
            skip = true;
            continue;
        }
        new_text.push(ch);
    }
    new_text
}
