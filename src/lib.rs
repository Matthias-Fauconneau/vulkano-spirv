#![feature(proc_macro_quote, iterator_try_collect)]
use vulkano::shader::spirv::{Spirv, Id, Instruction::{Name, MemberName, TypeFloat, TypeInt, TypeVector, TypeMatrix, TypeStruct}};

fn name_from_id(spirv: &Spirv, id: Id) -> Option<&str> {
	spirv.id(id).names().iter().find_map(|instruction| if let Name{name, .. } = instruction { Some(name.as_str()) } else { None }) 
}

type Result<T,E=Box<dyn std::error::Error>> = std::result::Result<T, E>;

fn r#type(spirv: &Spirv, id: Id) -> Result<TokenStream> {
	Ok(match spirv.id(id).instruction() {
		TypeFloat{width: 16, ..} => quote!(f16),
		TypeFloat{width: 32, ..} => quote!(f32),
		TypeInt{width: 32, ..} => quote!(u32),
		&TypeVector{component_count, component_type, ..} => {
			let component_type = r#type(spirv, component_type)?;
			let component_count = TokenTree::Literal(Literal::usize_unsuffixed(component_count as usize));
			quote!([$component_type; $component_count])
		},
		&TypeMatrix{column_count, column_type, ..} => {
			let &TypeVector{component_count: row_count, component_type, ..} = spirv.id(column_type).instruction() else {unimplemented!("column type")};
			let row_count = match row_count { 3=>4, 4=>4, _=>unimplemented!("row count") };
			let [row_count, column_count] = [row_count, column_count].map(|v| TokenTree::Literal(Literal::usize_unsuffixed(v as usize)));
			let component_type = r#type(spirv, component_type)?;
			quote!([[$component_type; $row_count]; $column_count])
		},
		TypeStruct{..} => TokenTree::Ident(Ident::new(name_from_id(spirv, id).unwrap(), Span::call_site())).into(),
		_ => unimplemented!("Unimplemented type conversion (SPIR-V => Rust)")
	})
}

fn format(spirv: &Spirv, r#type: Id) -> TokenStream {
	fn format(spirv: &Spirv, r#type: Id, component_count: u32) -> TokenStream {
		fn format(r#type: &str, width: u32, component_count: u32) -> TokenStream {
			fn format(format: impl AsRef<str>) -> TokenStream {
				let format = TokenTree::Ident(Ident::new(format.as_ref(), Span::call_site()));
				quote!(#[format($format)])
			}
			let width = width.to_string();
			format(["R","G","B","A"][..component_count as usize].iter().map(|component| [component, width.as_str()].into_iter()).flatten().collect::<String>()+"_"+r#type)
		}
		let (r#type, width) = match spirv.id(r#type).instruction() { 
			&TypeInt{width, ..} => ("UINT", width), 
			&TypeFloat{width, ..} => ("SFLOAT", width), 
			_ => unimplemented!("type")
		};
		format(r#type, width, component_count)
	}
	if let &TypeVector{component_count, component_type, ..} = spirv.id(r#type).instruction() { format(spirv, component_type, component_count) } 
	else { format(spirv, r#type, 1) }
}

struct MacroInput(syn::Ident);
impl syn::parse::Parse for MacroInput { 
	fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> { 
		Ok(Self(input.parse::<syn::Ident>()?)) 
	}
}

use proc_macro::{TokenTree, TokenStream, Ident, Literal, Span, quote};

#[proc_macro] pub fn shader(input: TokenStream) -> TokenStream { 
	shader_proc(syn::parse_macro_input!(input as MacroInput)).unwrap() 
}

fn shader_proc(MacroInput(name): MacroInput) -> Result<TokenStream> {
	let ref spirv = Spirv::new(bytemuck::cast_slice(&std::fs::read(&std::path::Path::new(&std::env::var("OUT_DIR")?).join(name.to_string()+".spv"))?))?;
	let mut struct_names = vec![];
	let structs = TokenStream::from_iter(spirv.types().iter().filter_map(|spirv_struct| {
		let TypeStruct{result_id: struct_id, member_types} = spirv_struct else { return None; };
		let struct_name = name_from_id(spirv, *struct_id)?;
		struct_names.push(struct_name);
		let members = member_types.iter().zip(spirv.id(*struct_id).members()).map(|(&member_type, field)| -> Result<TokenStream> {
			let format = (struct_name == "Vertex").then(|| format(spirv, member_type));
			let member_name = field.names().iter().find_map(|instruction| {
				let MemberName { name, .. } = instruction else { return None };
				Some(TokenTree::Ident(Ident::new(name, Span::call_site())))
			}).unwrap();
			let member_type = r#type(spirv, member_type)?;
			Ok(quote!($format pub $member_name: $member_type,))
		}).try_collect::<TokenStream>().unwrap();
		let vertex = (struct_name == "Vertex").then_some(quote!(vulkano::pipeline::graphics::vertex_input::Vertex));
		let struct_name = TokenTree::Ident(Ident::new(struct_name, Span::call_site()));
		// bytemuck::Pod => vulkano::buffer::subbuffer::BufferContents
		//Some(quote!{#[repr(C)]#[derive(Clone,Copy,self::bytemuck::Zeroable,self::bytemuck::Pod,$vertex)] pub struct $struct_name { $members }})
		Some(quote!{#[repr(C)]#[derive(Clone,Copy,vulkano::buffer::subbuffer::BufferContents,$vertex)] pub struct $struct_name { $members }})
	}));
	let uniforms = TokenTree::Ident(Ident::new("Uniforms", Span::call_site()));
	let empty = TokenTree::Ident(Ident::new("empty", Span::call_site()));
	let maybe_define_empty_uniforms_if_not_defined = (!struct_names.contains(&"Uniforms")).then_some(quote!{
		#[repr(C)]#[derive(Clone,Copy,bytemuck::AnyBitPattern)] pub struct $uniforms(u8);
		impl $uniforms { pub fn $empty() -> Self { Self(0) } }
	});
	let vertex = TokenTree::Ident(Ident::new("Vertex", Span::call_site()));
	let maybe_define_empty_vertex_if_not_defined = (!struct_names.contains(&"Vertex")).then_some(quote!{
		#[repr(C)]#[derive(Clone,Copy,bytemuck::AnyBitPattern,vulkano::pipeline::graphics::vertex_input::Vertex)] pub struct $vertex {}
	});
	Ok(quote!{$maybe_define_empty_uniforms_if_not_defined $maybe_define_empty_vertex_if_not_defined $structs})
}
