use std::{
    cell::RefCell, collections::HashMap, iter::{zip, FromIterator}, rc::Rc
};

use js_sys::{Array, Uint8Array};
use wasm_bindgen::{prelude::*, throw_str};
use web_sys::{
    window, WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader, WebGlUniformLocation, WebGlVertexArrayObject
};

pub fn perspective_matrix(fov: f32, aspect_ratio: f32, near: f32, far: f32) -> [f32; 16] {
    // https://developer.mozilla.org/en-US/docs/Web/API/WebGL_API/WebGL_model_view_projection
    let f = 1. / (fov / 2.).tan();
    let range = 1. / (near - far);

    [
        f / aspect_ratio, 0., 0., 0.,
        0., f, 0., 0.,
        0., 0., (near + far) * range, -1.,
        0., 0., near * far * range * 2., 0.,
    ]
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

fn compile_shader(
    context: &WebGl2RenderingContext,
    shader_type: u32,
    source: &str,
) -> Result<WebGlShader, String> {
    let shader = context
        .create_shader(shader_type)
        .ok_or_else(|| String::from("Unable to create shader object"))?;
    context.shader_source(&shader, source);
    context.compile_shader(&shader);

    if context
        .get_shader_parameter(&shader, WebGl2RenderingContext::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(shader)
    } else {
        Err(context
            .get_shader_info_log(&shader)
            .unwrap_or_else(|| String::from("Unknown error creating shader")))
    }
}

fn link_program(
    context: &WebGl2RenderingContext,
    vert_shader: &WebGlShader,
    frag_shader: &WebGlShader,
) -> Result<WebGlProgram, String> {
    let program = context
        .create_program()
        .ok_or_else(|| String::from("Unable to create shader object"))?;

    context.attach_shader(&program, vert_shader);
    context.attach_shader(&program, frag_shader);
    context.link_program(&program);

    if context
        .get_program_parameter(&program, WebGl2RenderingContext::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        Ok(program)
    } else {
        Err(context
            .get_program_info_log(&program)
            .unwrap_or_else(|| String::from("Unknown error creating program object")))
    }
}

trait OrThrow<T> {
    fn or_throw(self) -> T;
}

impl<T> OrThrow<T> for Result<T, String> {
    fn or_throw(self) -> T {
        if self.is_err() {
            throw_str(&self.err().unwrap())
        } else {
            self.unwrap()
        }
    }
}

pub struct Shader {
    program: WebGlProgram,
    attribute_locations: HashMap<String, u32>,
    uniform_locations: HashMap<String, WebGlUniformLocation>,
}

impl Shader {
    pub fn new(
        context: &WebGl2RenderingContext,
        vertex_src: &str,
        fragment_src: &str,
        uniforms: &[&str],
        attributes: &[&str],
    ) -> Shader {
        let vert_shader = compile_shader(
            context,
            WebGl2RenderingContext::VERTEX_SHADER,
            vertex_src
        ).or_throw();
        let frag_shader = compile_shader(
            context,
            WebGl2RenderingContext::FRAGMENT_SHADER,
            fragment_src,
        ).or_throw();
        let program = link_program(context, &vert_shader, &frag_shader)
            .or_throw();
        context.delete_shader(Some(&vert_shader));
        context.delete_shader(Some(&frag_shader));
        Shader {
            attribute_locations: HashMap::from_iter(attributes.iter().map(|attr| {
                (
                    String::from(*attr),
                    context.get_attrib_location(&program, attr) as u32,
                )
            })),
            uniform_locations: HashMap::from_iter(uniforms.iter().map(|attr| {
                (
                    String::from(*attr),
                    context.get_uniform_location(&program, attr).unwrap(),
                )
            })),
            program,
        }
    }

    pub fn find_attr(&self, name: &str) -> u32 {
        return self.attribute_locations[name];
    }

    pub fn find_uniform(&self, name: &str) -> &WebGlUniformLocation {
        return &self.uniform_locations[name];
    }

    pub fn enable(&self, context: &WebGl2RenderingContext) {
        context.use_program(Some(&self.program));
    }
}

pub struct VBO<T> {
    pub buffer: Vec<T>,
    handle: WebGlBuffer,
    buffer_type: u32,
    access_type: u32,
}

macro_rules! VBO_bind {
    ($vbo:expr, $ctx:expr, $shader:expr, $DataClass:ty, $member:ident, $sz:expr, $type:expr) => {
        $vbo.bind(
            $ctx,
            $shader.find_attr(stringify!($member)),
            $sz,
            $type,
            false,
            std::mem::offset_of!($DataClass, $member)
        )
    };
    ($vbo:expr, $ctx:expr, $addr:expr, $DataClass:ty, $sz:expr, $type:expr) => {
        $vbo.bind(
            $ctx,
            $addr,
            $sz,
            $type,
            false,
            0
        )
    };
    ($vbo:expr, $ctx:expr, $shader:expr, $DataClass:ty, $member:ident, $sz:expr, $type:expr, $normalized:expr) => {
        $vbo.bind(
            $ctx,
            $shader.find_attr(stringify!($member)),
            $sz,
            $type,
            $normalized,
            std::mem::offset_of!($DataClass, $member)
        )
    };
    ($vbo:expr, $ctx:expr, $addr:expr, $DataClass:ty, $sz:expr, $type:expr, $normalized:expr) => {
        $vbo.bind(
            $ctx,
            $addr,
            $sz,
            $type,
            $normalized,
            0
        )
    };
}

impl<T> VBO<T> {
    pub fn new(ctx: &WebGl2RenderingContext, data: Option<Vec<T>>, buffer_type: u32, access_type: u32) -> VBO<T> {
        let result = VBO {
            buffer: data.unwrap_or_default(),
            handle: ctx.create_buffer().expect_throw("Failed to create buffer"),
            buffer_type,
            access_type,
        };
        result.update(ctx);
        result
    }

    pub fn update(&self, ctx: &WebGl2RenderingContext) {
        ctx.bind_buffer(self.buffer_type, Some(&self.handle));
        // Note that `Float32Array::view` is somewhat dangerous (hence the
        // `unsafe`!). This is creating a raw view into our module's
        // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
        // (aka do a memory allocation in Rust) it'll cause the buffer to change,
        // causing the `Float32Array` to be invalid.
        //
        // As a result, after `Float32Array::view` we have to be very careful not to
        // do any memory allocations before it's dropped.
        unsafe {
            ctx.buffer_data_with_array_buffer_view(
                self.buffer_type,
                &Uint8Array::view(self.buffer.as_slice().align_to::<u8>().1),
                self.access_type,
            );
        }
    }

    pub fn bind(&self, ctx: &WebGl2RenderingContext, 
            addr: u32, size: i32, type_: u32, normalized: bool, offset: usize) {
        ctx.bind_buffer(
            self.buffer_type,
            Some(&self.handle),
        );
        ctx.vertex_attrib_pointer_with_i32(
            addr,
            size,
            type_,
            normalized,
            std::mem::size_of::<T>() as i32,
            offset as i32,
        );
        ctx.enable_vertex_attrib_array(addr);
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

pub struct VAO<T> {
    pub handle: WebGlVertexArrayObject,
    pub vbos: Box<T>,
}

macro_rules! VAO_new {
    ($ctx:expr, $(($vbo:expr, $buffer_type:expr, $access_type:expr)),*) => {{
        let ctx: &WebGl2RenderingContext = $ctx;
        let handle = ctx.create_vertex_array()
            .expect_throw("Could not create vertex array object");
        ctx.bind_vertex_array(Some(&handle));
        crate::renderer::VAO {
            handle,
            vbos: Box::new((
                $(
                    crate::renderer::VBO::new(ctx, Some($vbo), $buffer_type, $access_type)
                ),*
            ))
        }
    }};
}

impl<T> VAO<T> {
    pub fn activate(&self, ctx: &WebGl2RenderingContext) {
        ctx.bind_vertex_array(Some(&self.handle));
    }
}


pub fn render_loop(mut callback: impl FnMut(bool) + 'static) -> Result<(), JsValue> {
    callback(true);
    let ref1 = Rc::new(RefCell::new(callback));
    let ref2 = ref1.clone();

    let init_cb = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
    let loop_cb = init_cb.clone();
    *init_cb.borrow_mut() = Some(Closure::new(move || {
        ref1.borrow_mut()(false);
        request_animation_frame(&loop_cb.borrow_mut().as_ref().unwrap());
    }));
    request_animation_frame(&init_cb.borrow_mut().as_ref().unwrap());
    let cb = Closure::<dyn FnMut()>::new(move || {
        ref2.borrow_mut()(true);
    });
    window().unwrap().add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())?;
    cb.forget();
    Ok(())
}
