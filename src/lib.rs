use std::{borrow::Borrow, cell::RefCell, collections::HashMap, iter::{FromIterator, Map}, ops::Deref, rc::Rc};

use wasm_bindgen::prelude::*;
use web_sys::{Event, HtmlCanvasElement, WebGl2RenderingContext, WebGlProgram, WebGlShader, Window};

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window().unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame` OK");
}

pub fn compile_shader(
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

pub fn link_program(
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

fn shader_program(
        context: &WebGl2RenderingContext,
        vertex_src: &str,
        fragment_src: &str
    ) -> Result<WebGlProgram, String> {
    let vert_shader = compile_shader(
        context,
        WebGl2RenderingContext::VERTEX_SHADER,
        vertex_src,
    )?;

    let frag_shader = compile_shader(
        context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        fragment_src,
    )?;
    Ok(link_program(context, &vert_shader, &frag_shader)?)
}

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy)]
struct Vertex {
    pos: Position,
}

struct Renderer {
    window: &'static Window,
    canvas: &'static HtmlCanvasElement,
    context: WebGl2RenderingContext,
    main_shader: WebGlProgram,
    vertices: Vec<Vertex>,
    attr_locations: HashMap<String, i32>,
}

impl Renderer {
    pub fn new(window: &'static Window, canvas: &'static HtmlCanvasElement) -> Result<Renderer, JsValue> {
        let context = canvas
                .get_context("webgl2")?
                .unwrap()
                .dyn_into::<WebGl2RenderingContext>()?;
        let shader_prog = shader_program(
                &context,
                r##"#version 300 es
 
                in vec4 position;

                void main() {
                    gl_Position = position;
                }
                "##,
                r##"#version 300 es
                
                precision highp float;
                out vec4 outColor;
                
                void main() {
                    outColor = vec4(1, 1, 1, 1);
                }
                "##)?;
        Ok(Renderer{
            window,
            canvas,
            vertices: [ //[-0.7, -0.7, 0.0, 0.7, -0.7, 0.0, 0.0, 0.7, 0.0]
                Vertex{pos: Position{x: -0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.0, y: 0.7, z: 0.0}},
            ].to_vec(),
            attr_locations: HashMap::from_iter(["position"].into_iter().map(|attr| {
                (String::from(*attr), context.get_attrib_location(&shader_prog, attr))
            })),
            context,
            main_shader: shader_prog,
        })
    }

    pub fn init(&'static self) {
        {
            let callback = Closure::<dyn FnMut(_)>::new(move |_event: Event| {
                self.resize_canvas();
                self.render();
            });
            self.window.add_event_listener_with_callback("resize", callback.as_ref().unchecked_ref()).expect_throw("add event listener failed");
            callback.forget();
        }
        self.render_loop();
    }

    fn resize_canvas(&self) {
        unsafe {
            self.canvas.set_width(self.window.inner_width().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
            self.canvas.set_height(self.window.inner_height().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
        }
    }

    fn render(&self) {
        self.context.clear_color(0.0, 0.0, 0.0, 1.0);
        self.context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

        self.context.draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, vert_count);
    }

    pub fn render_loop(&'static self) {
        let f = Rc::new(RefCell::new(None::<Closure::<dyn FnMut()>>));
        let g = f.clone();
        *g.borrow_mut() = Some(Closure::new(move || {
            self.render();
            // Schedule ourself for another requestAnimationFrame callback.
            request_animation_frame(f.as_ref().borrow().as_ref().unwrap())
        }));
        request_animation_frame(g.as_ref().borrow().as_ref().unwrap());
    }
}

#[wasm_bindgen(start)]
fn start() -> Result<(), JsValue> {
    let window: Window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;
    let renderer = Renderer::new(&window, &canvas)?;

    let vertices: [f32; 9] = [-0.7, -0.7, 0.0, 0.7, -0.7, 0.0, 0.0, 0.7, 0.0];

    let position_attribute_location = context.get_attrib_location(&program, "position");
    let buffer = context.create_buffer().ok_or("Failed to create buffer")?;
    context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));

    // Note that `Float32Array::view` is somewhat dangerous (hence the
    // `unsafe`!). This is creating a raw view into our module's
    // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
    // (aka do a memory allocation in Rust) it'll cause the buffer to change,
    // causing the `Float32Array` to be invalid.
    //
    // As a result, after `Float32Array::view` we have to be very careful not to
    // do any memory allocations before it's dropped.
    unsafe {
        let positions_array_buf_view = js_sys::Float32Array::view(&vertices);

        context.buffer_data_with_array_buffer_view(
            WebGl2RenderingContext::ARRAY_BUFFER,
            &positions_array_buf_view,
            WebGl2RenderingContext::STATIC_DRAW,
        );
    }

    let vao = context
        .create_vertex_array()
        .ok_or("Could not create vertex array object")?;
    context.bind_vertex_array(Some(&vao));

    context.vertex_attrib_pointer_with_i32(
        position_attribute_location as u32,
        3,
        WebGl2RenderingContext::FLOAT,
        false,
        0,
        0,
    );
    context.enable_vertex_attrib_array(position_attribute_location as u32);

    context.bind_vertex_array(Some(&vao));

    let vert_count = (vertices.len() / 3) as i32;
    

    Ok(())
}

fn draw(context: &WebGl2RenderingContext, vert_count: i32) {
    context.clear_color(0.0, 0.0, 0.0, 1.0);
    context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

    context.draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, vert_count);
}