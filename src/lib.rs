use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Fields, Ident, Path, Visibility, parse::Parser,
    parse_macro_input, parse_quote, punctuated::Punctuated, token::Comma,
};

#[proc_macro_attribute]
pub fn tomlua_config(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let struct_name: &Ident = &input.ident;
    let vis: &Visibility = &input.vis;

    let data = match &input.data {
        Data::Struct(s) => s,
        _ => {
            return syn::Error::new_spanned(
                input,
                "#[tomlua_config] attribute can only be used on structs",
            )
            .to_compile_error()
            .into();
        }
    };

    let mut fields = match &data.fields {
        Fields::Named(named) => named.named.clone(),
        _ => {
            return syn::Error::new_spanned(
                input,
                "#[tomlua_config] attribute does not support unnamed fields",
            )
            .into_compile_error()
            .into();
        }
    };

    fields.push(
        syn::Field::parse_named
            .parse2(quote! { pub scripts: ::std::vec::Vec<::tomlua::Script> })
            .unwrap(),
    );

    let mut attrs: Vec<Attribute> = input.attrs.clone();
    let mut derive_found = false;
    let mut new_attrs: Vec<Attribute> = Vec::new();
    for attr in &mut attrs {
        if attr.path().is_ident("derive") {
            derive_found = true;
            let nested: Punctuated<Path, Comma> = attr
                .parse_args_with(Punctuated::<Path, Comma>::parse_terminated)
                .unwrap_or_default();

            let mut new_nested = nested.clone();
            new_nested.push(parse_quote!(TomluaExecute));

            let new_attr: Attribute = parse_quote!(#[derive(#new_nested)]);
            new_attrs.push(new_attr);
        } else {
            new_attrs.push(attr.clone());
        }
    }

    if !derive_found {
        new_attrs.push(parse_quote!(#[derive(TomluaExecute)]))
    }

    let expanded = quote! {
        use ::tomlua::Script;

        #(#new_attrs)*
        #vis struct #struct_name {
            #fields
        }
    };

    expanded.into()
}

#[proc_macro_derive(TomluaExecute)]
pub fn tomula_execute_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = if let syn::Data::Struct(ref data) = input.data {
        if let syn::Fields::Named(ref named) = data.fields {
            named.named.iter().map(|f| &f.ident).collect::<Vec<_>>()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let expanded = quote! {
        use ::mlua::Lua;
        impl #name {
            fn extract_lua_code(script: &Script) -> Result<String, String> {
                match &script.inline() {
                    Some(code) => return Ok(code.to_string()),
                    None => {},
                }
                match &script.path() {
                    Some(p) => {
                        return std::fs::read_to_string(p)
                                .map_err(|e| format!("Failed to read script `{}`:\n{}", script.name(), e.to_string()))
                    },
                    None => {},
                }
                Err(format!("No code found for '{}'\nexpected at least one of: 'inline', 'path'", script.name()))
            }

            pub fn execute_script(&self, script_name: &str) -> Result<Lua, String> {
                let lua = Lua::new();
                let globals = lua.globals();
                #(globals.set(
                    stringify!(#fields),
                    self.#fields.clone())
                        .map_err(|e| format!("Failed to inject fields as Lua variables\n{}", e.to_string())
                )?;)*

                if let Some(script) = self.scripts.iter().find(|s| s.name() == script_name) {
                    let lua_script_str = #name::extract_lua_code(&script)?;
                    lua.load(lua_script_str).exec()
                        .map_err(|e| format!("Scrip `{}` failed: {}", script.name(), e.to_string()))?;
                }
                Ok(lua)
            }

            pub fn execute_all(&self) -> Result<Lua, String> {
                let lua = Lua::new();
                let globals = lua.globals();
                #(globals.set(
                    stringify!(#fields),
                    self.#fields.clone())
                        .map_err(|e| format!("Failed to inject fields as Lua variables\n{}", e.to_string())
                )?;)*

                for script in &self.scripts {
                    let lua_script_str = #name::extract_lua_code(&script)?;
                    lua.load(lua_script_str).exec()
                        .map_err(|e| format!("Scrip `{}` failed: {}", script.name(), e.to_string()))?;
                }
                Ok(lua)
            }

            pub fn update(&mut self, lua: Lua) -> Result<(), String> {
                let globals = lua.globals();

                #(
                    self.#fields = globals
                        .get::<_>(stringify!(#fields))
                        .map_err(|e| format!(
                            "Failed to read global `{}` from Lua: {}",
                            stringify!(#fields),
                            e
                        ))?;
                )*

                Ok(())
            }
        }
    };

    expanded.into()
}
