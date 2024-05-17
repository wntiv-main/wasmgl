use std::{any::Any, borrow::Borrow, cell::{Cell, RefCell}, collections::HashMap, default, iter::FromIterator, mem::{forget, offset_of, size_of}, rc::Rc, sync::{atomic::{AtomicPtr, Ordering}, Arc, Mutex, RwLock}};

use once_cell::{race::{OnceBox, OnceRef}, sync::{Lazy, OnceCell}};
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

struct Renderer2 {
    window: OnceCell<Window>,
    canvas: OnceCell<HtmlCanvasElement>,
    context: OnceCell<WebGl2RenderingContext>,
    main_shader: OnceCell<WebGlProgram>,
    vertices: Vec<Vertex>,
    attr_locations: HashMap<String, u32>,
}

unsafe impl Send for Renderer2 {}
unsafe impl Sync for Renderer2 {}

impl Renderer2 {
    pub fn new() -> Result<Renderer2, JsValue> {
        Ok(Renderer2{
            vertices: vec![
                Vertex{pos: Position{x: -0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.0, y: 0.7, z: 0.0}},
            ],
            attr_locations: HashMap::new(),
            window: Default::default(),
            canvas: Default::default(),
            context: Default::default(),
            main_shader: Default::default(),
        })
    }

    pub fn init(&mut self, window: Window, canvas: HtmlCanvasElement) {
        let ctx = canvas
        .get_context("webgl2")
                .expect("webgl2 rendering ctx")
                .unwrap()
                .dyn_into::<WebGl2RenderingContext>().expect("webgl2 rendering ctx");
        self.canvas.set(canvas).expect("only call init() once");
        let shader_prog = shader_program(
            &ctx,
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
            "##).expect("shader should compile");
            self.attr_locations.extend(["position"].iter().map(|attr| {
                (String::from(*attr), ctx.get_attrib_location(&shader_prog, attr) as u32)
            }));
            self.main_shader.set(shader_prog).expect("only call init() once");
            
            let buffer = ctx.create_buffer().expect_throw("Failed to create buffer");
            ctx.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));

            let vao = ctx
                .create_vertex_array()
                .expect_throw("Could not create vertex array object");
            ctx.bind_vertex_array(Some(&vao));
            
            self.window.set(window).expect("only call init() once");
            self.context.set(ctx).expect("only call init() once");

            self.update();
            
        }
        
        fn vbo(&self, buf: &[f32], attribute_location: u32, offset: i32, dynamic: bool) {
            // Note that `Float32Array::view` is somewhat dangerous (hence the
            // `unsafe`!). This is creating a raw view into our module's
            // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
            // (aka do a memory allocation in Rust) it'll cause the buffer to change,
            // causing the `Float32Array` to be invalid.
            //
            // As a result, after `Float32Array::view` we have to be very careful not to
            // do any memory allocations before it's dropped.
            unsafe {
                let array_buf_view = js_sys::Float32Array::view(buf);

                self.context.get().unwrap().buffer_data_with_array_buffer_view(
                    WebGl2RenderingContext::ARRAY_BUFFER,
                    &array_buf_view,
                    if dynamic {WebGl2RenderingContext::STATIC_DRAW} else {WebGl2RenderingContext::DYNAMIC_DRAW},
                );
            }

            self.context.get().unwrap().vertex_attrib_pointer_with_i32(
                attribute_location,
                (size_of::<Position>() / size_of::<f32>()) as i32,
                WebGl2RenderingContext::FLOAT,
                false,
                size_of::<Vertex>() as i32,
                offset,
            );
            self.context.get().unwrap().enable_vertex_attrib_array(attribute_location);
        }

        fn resize_canvas(&self) {
            unsafe {
                self.canvas.get().unwrap().set_width(
                    self.window.get().unwrap().inner_width().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
                self.canvas.get().unwrap().set_height(
                    self.window.get().unwrap().inner_height().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
            }
        }

        fn update(&self) {
            self.vbo(
                unsafe{self.vertices.as_slice().align_to::<f32>().1},
                self.attr_locations["position"],
                offset_of!(Vertex, pos) as i32,
                false);
        }

        fn render(&self) {
            self.context.get().unwrap().clear_color(0.0, 0.0, 0.0, 1.0);
            self.context.get().unwrap().clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

            self.context.get().unwrap().draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, self.vertices.len() as i32);
        }

        pub fn render_loop(&'static self) {
            {
                let callback = Closure::<dyn FnMut(_)>::new(move |_event: Event| {
                    (&self).resize_canvas();
                    (&self).render();
                });
                self.window.get().unwrap().add_event_listener_with_callback("resize", callback.as_ref().unchecked_ref()).expect_throw("add event listener failed");
                callback.forget();
            }

            let f = Rc::new(RefCell::new(None::<Closure::<dyn FnMut()>>));
            let g = f.clone();
            *g.borrow_mut() = Some(Closure::new(move || {
                (&self).render();
                // Schedule ourself for another requestAnimationFrame callback.
                request_animation_frame(f.as_ref().borrow().as_ref().unwrap())
            }));
            request_animation_frame(g.as_ref().borrow().as_ref().unwrap());
        }
    }

static RENDERER2: Lazy<Arc<RwLock<Renderer2>>> = Lazy::new(|| {Arc::new(RwLock::new(
    Renderer2::new().expect("renderer obj")))});

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
    window: Window,
    canvas: HtmlCanvasElement,
    context: WebGl2RenderingContext,
    main_shader: WebGlProgram,
    vertices: Vec<Vertex>,
    attr_locations: HashMap<String, u32>,
}

impl Renderer {
    pub fn new(window: Window, canvas: HtmlCanvasElement) -> Result<Renderer, JsValue> {
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
            vertices: [
                Vertex{pos: Position{x: -0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.7, y: -0.7, z: 0.0}},
                Vertex{pos: Position{x: 0.0, y: 0.7, z: 0.0}},
            ].to_vec(),
            attr_locations: HashMap::from_iter(["position"].iter().map(|attr| {
                (String::from(*attr), context.get_attrib_location(&shader_prog, attr) as u32)
            })),
            context,
            main_shader: shader_prog,
        })
    }

    pub fn init(&'static self) {
        let buffer = self.context.create_buffer().expect_throw("Failed to create buffer");
        self.context.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&buffer));

        {
            let callback = Closure::<dyn FnMut(_)>::new(move |_event: Event| {
                self.resize_canvas();
                self.render();
            });
            self.window.add_event_listener_with_callback("resize", callback.as_ref().unchecked_ref()).expect_throw("add event listener failed");
            callback.forget();
        }

        self.update();

        let vao = self.context
            .create_vertex_array()
            .expect_throw("Could not create vertex array object");
        self.context.bind_vertex_array(Some(&vao));

        self.render_loop();
    }

    fn vbo(&self, buf: &[f32], attribute_location: u32, offset: i32, dynamic: bool) {
        // Note that `Float32Array::view` is somewhat dangerous (hence the
        // `unsafe`!). This is creating a raw view into our module's
        // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
        // (aka do a memory allocation in Rust) it'll cause the buffer to change,
        // causing the `Float32Array` to be invalid.
        //
        // As a result, after `Float32Array::view` we have to be very careful not to
        // do any memory allocations before it's dropped.
        unsafe {
            let array_buf_view = js_sys::Float32Array::view(buf);

            self.context.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &array_buf_view,
                if dynamic {WebGl2RenderingContext::STATIC_DRAW} else {WebGl2RenderingContext::DYNAMIC_DRAW},
            );
        }

        self.context.vertex_attrib_pointer_with_i32(
            attribute_location,
            (size_of::<Position>() / size_of::<f32>()) as i32,
            WebGl2RenderingContext::FLOAT,
            false,
            size_of::<Vertex>() as i32,
            offset,
        );
        self.context.enable_vertex_attrib_array(attribute_location);
    }

    fn resize_canvas(&self) {
        unsafe {
            self.canvas.set_width(self.window.inner_width().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
            self.canvas.set_height(self.window.inner_height().unwrap().as_f64().unwrap_or_default().to_int_unchecked::<u32>());
        }
    }

    fn update(&self) {
        self.vbo(
            unsafe{self.vertices.as_slice().align_to::<f32>().1},
            self.attr_locations["position"],
            offset_of!(Vertex, pos) as i32,
            false);
    }

    fn render(&self) {
        self.context.clear_color(0.0, 0.0, 0.0, 1.0);
        self.context.clear(WebGl2RenderingContext::COLOR_BUFFER_BIT);

        self.context.draw_arrays(WebGl2RenderingContext::TRIANGLES, 0, self.vertices.len() as i32);
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

static RENDERER: OnceCell<AtomicPtr<Renderer>> = OnceCell::new();

#[wasm_bindgen(start)]
fn start() -> Result<(), JsValue> {
    let window: Window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into::<web_sys::HtmlCanvasElement>()?;
    // let mut renderer = Renderer::new(window, canvas).unwrap();
    // RENDERER.set(AtomicPtr::new(&mut renderer)).expect("ok");
    // unsafe {
    //     RENDERER.get().unwrap().load(Ordering::Relaxed).as_ref().unwrap().init();
    // }
    { RENDERER2.write().unwrap().init(window, canvas) };
    RENDERER2.read().and_then(|res| {
        res.render_loop();
        Ok(())
    });

    Ok(())
}
