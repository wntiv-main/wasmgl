use std::{
    any::Any, borrow::{self, BorrowMut}, cell::RefCell, collections::HashMap, iter::FromIterator, marker::Tuple, mem::{offset_of, size_of}, ops::{DerefMut, Div}, rc::Rc
};

use js_sys::{ArrayBuffer, Float32Array, SharedArrayBuffer, Uint8Array};
use wasm_bindgen::{convert::VectorIntoWasmAbi, prelude::*};
use web_sys::{
    Event, HtmlCanvasElement, WebGl2RenderingContext, WebGlBuffer, WebGlProgram, WebGlShader,
    Window,
};

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
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
    fragment_src: &str,
) -> Result<WebGlProgram, String> {
    let vert_shader = compile_shader(context, WebGl2RenderingContext::VERTEX_SHADER, vertex_src)?;

    let frag_shader = compile_shader(
        context,
        WebGl2RenderingContext::FRAGMENT_SHADER,
        fragment_src,
    )?;
    link_program(context, &vert_shader, &frag_shader)
}

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone, Copy)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
}

#[derive(Clone, Copy)]
struct Vertex {
    pos: Position,
    color: Color,
}

trait IVBO {
    fn len(&self) -> usize;
    fn update(&mut self, ctx: &WebGl2RenderingContext);
}

struct VBO<T> {
    source: Vec<T>,
    buffer: WebGlBuffer,
}

impl<T> VBO<T> {
    pub fn new(ctx: &WebGl2RenderingContext, default_content: Option<Vec<T>>) -> VBO<T> {
        VBO {
            buffer: ctx.create_buffer().expect_throw("Failed to create buffer"),
            source: default_content.unwrap_or_default(),
        }
    }

    pub fn bind(&self, ctx: &WebGl2RenderingContext, 
            addr: u32, size: i32, type_: u32, normalized: bool, offset: usize) {
        ctx.bind_buffer(
            WebGl2RenderingContext::ARRAY_BUFFER,
            Some(&self.buffer),
        );
        ctx.vertex_attrib_pointer_with_i32(
            addr,
            size,
            type_,
            normalized,
            size_of::<T>() as i32,
            offset as i32,
        );
        ctx.enable_vertex_attrib_array(addr);
    }
}

impl<T> IVBO for VBO<T> {
    fn len(&self) -> usize {
        self.source.len()
    }

    fn update(&mut self, ctx: &WebGl2RenderingContext) {
        ctx.bind_buffer(WebGl2RenderingContext::ARRAY_BUFFER, Some(&self.buffer));
        // Note that `Float32Array::view` is somewhat dangerous (hence the
        // `unsafe`!). This is creating a raw view into our module's
        // `WebAssembly.Memory` buffer, but if we allocate more pages for ourself
        // (aka do a memory allocation in Rust) it'll cause the buffer to change,
        // causing the `Float32Array` to be invalid.
        //
        // As a result, after `Float32Array::view` we have to be very careful not to
        // do any memory allocations before it's dropped.
        unsafe {
            // let array_buf_view =
            //     js_sys::Float32Array::view(self.source.as_slice().align_to::<f32>().1);

            ctx.buffer_data_with_array_buffer_view(
                WebGl2RenderingContext::ARRAY_BUFFER,
                &Float32Array::view(self.source.as_slice().align_to::<f32>().1),
                WebGl2RenderingContext::STATIC_DRAW,
            );
        }
    }
}

struct Renderer {
    window: Window,
    canvas: HtmlCanvasElement,
    context: WebGl2RenderingContext,
    main_shader: WebGlProgram,
    vbos: Vec<Box<dyn IVBO>>,
    attr_locations: HashMap<String, u32>,
    timer: u32,
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
 
                in vec3 position;
                in vec3 vertexColor;
                out vec3 color;

                void main() {
                    color = vertexColor;
                    gl_Position = vec4(position, 1);
                }
                "##,
            r##"#version 300 es
                
                precision highp float;

                in vec3 color;
                out vec4 outColor;
                
                void main() {
                    outColor = vec4(color, 1);
                }
                "##,
        )?;

        Ok(Renderer {
            window,
            canvas,
            vbos: Default::default(),
            attr_locations: HashMap::from_iter(["position", "vertexColor"].iter().map(|attr| {
                (
                    String::from(*attr),
                    context.get_attrib_location(&shader_prog, attr) as u32,
                )
            })),
            context,
            main_shader: shader_prog,
            timer: 0,
        })
    }

    pub fn init(&mut self) {
        self.context.use_program(Some(&self.main_shader));

        let vao = self
            .context
            .create_vertex_array()
            .expect_throw("Could not create vertex array object");
        self.context.bind_vertex_array(Some(&vao));

        let mut vbo = VBO::new(
        &self.context,
            Some(vec![
                Vertex { pos: Position { x: -0.7, y: -0.7, z: 0.0 }, color: Color { r: 1.0, g: 0.0, b: 0.0 } },
                Vertex { pos: Position { x: 0.7, y: -0.7, z: 0.0 }, color: Color { r: 0.0, g: 1.0, b: 0.0 } },
                Vertex { pos: Position { x: 0.0, y: 0.7, z: 0.0 }, color: Color { r: 0.0, g: 0.0, b: 1.0 } },
            ]));
        // vbo.update(&self.context);
        vbo.bind(&self.context, self.attr_locations["position"],
            3, WebGl2RenderingContext::FLOAT, false, offset_of!(Vertex, pos));
        vbo.bind(&self.context, self.attr_locations["vertexColor"],
            3, WebGl2RenderingContext::FLOAT, false, offset_of!(Vertex, color));

        self.vbos.push(Box::new(vbo));
        
        // let mut pos_vbo = VBO::new(
        // &self.context,
        //     Some(vec![
        //         Position { x: -0.7, y: -0.7, z: 0.0 },
        //         Position { x: 0.7, y: -0.7, z: 0.0 },
        //         Position { x: 0.0, y: 0.7, z: 0.0 },
        //     ]));
        // pos_vbo.update(&self.context);
        // pos_vbo.bind(&self.context, self.attr_locations["position"],
        //     3, WebGl2RenderingContext::FLOAT, false, 0);
        
        // self.vbos.push(Box::new(pos_vbo));

        self.update();
            
        // let mut col_vbo = VBO::new(
        // &self.context,
        //     Some(vec![
        //         Color { r: 1.0, g: 0.0, b: 0.0 },
        //         Color { r: 0.0, g: 1.0, b: 0.0 },
        //         Color { r: 0.0, g: 0.0, b: 1.0 },
        //     ]));
        // col_vbo.update(&self.context);
        // col_vbo.bind(&self.context, self.attr_locations["vertexColor"],
        //     3, WebGl2RenderingContext::FLOAT, false, 0);

        // self.vbos.push(Box::new(col_vbo));
    }

    fn resize_canvas(&self) {
        unsafe {
            self.canvas.set_width(
                self.window
                    .inner_width()
                    .unwrap()
                    .as_f64()
                    .unwrap_or_default()
                    .to_int_unchecked::<u32>(),
            );
            self.canvas.set_height(
                self.window
                    .inner_height()
                    .unwrap()
                    .as_f64()
                    .unwrap_or_default()
                    .to_int_unchecked::<u32>(),
            );
        }
        self.context.viewport(
            0,
            0,
            self.canvas.width() as i32,
            self.canvas.height() as i32,
        );
    }

    fn update(&mut self) {
        for vbo in &mut self.vbos {
            vbo.update(&self.context);
        }
    }

    fn render(&mut self) {
        self.timer += 1;
        self.context.clear_color(0., 0., 0., 1.);
        self.context.clear(
            WebGl2RenderingContext::COLOR_BUFFER_BIT | WebGl2RenderingContext::DEPTH_BUFFER_BIT,
        );

        // let x: &mut (dyn Any + 'static) = self.vbos[0].deref_mut() as &mut dyn Any;
        // let y = &x.;
        // y.downcast_mut::()

        let vbo: &VBO<Vertex> = (<Box<dyn IVBO> as BorrowMut<dyn IVBO>>::borrow_mut(self.vbos[0]) as &mut dyn Any).downcast_ref::<VBO<Vertex>>().expect("IVBO is VBO<Vertex>");

        vbo.source[0].pos.x = (self.timer as f32).div(30.).sin();
        vbo.source[1].pos.x = -(self.timer as f32).div(30.).sin();

        self.context.draw_arrays(
            WebGl2RenderingContext::TRIANGLES,
            0,
            self.vbos[0].len() as i32,
        );
    }

    pub fn render_loop(mut callback: impl FnMut(bool) + 'static) {
        callback(true);
        let callback_ref: Rc<RefCell<dyn FnMut(bool)>> = Rc::new(RefCell::new(callback));
        let callback_ref2: Rc<RefCell<dyn FnMut(bool)>> = callback_ref.clone();

        let cb = Closure::<dyn FnMut(_)>::new(move |_event: Event| {
            {
                callback_ref.try_borrow_mut().unwrap()(true)
            };
        });
        web_sys::window()
            .unwrap()
            .add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref())
            .expect_throw("add event listener failed");
        cb.forget();

        let f = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
        let g = f.clone();
        *std::cell::RefCell::<_>::borrow_mut(&g) = Some(Closure::new(move || {
            {
                callback_ref2.try_borrow_mut().unwrap()(false)
            };
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
    let mut renderer = Renderer::new(window, canvas).unwrap();
    renderer.init();
    Renderer::render_loop(move |resize: bool| {
        if resize {
            renderer.resize_canvas();
        }
        renderer.render();
    });

    Ok(())
}
