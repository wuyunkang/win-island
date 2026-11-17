use crate::{bar_layout::*,paint::{self,Card,H},theme::{self,W}};
use eframe::egui::{self,Color32,RichText,Rect,Pos2,Vec2};

pub(crate) fn preview_layout(order:&[usize],quotas:usize,battery:bool,pet:bool,music:bool,focus:bool)->Layout {
    let mut layout=Layout::new(battery,quotas,music,focus,false,pet);
    layout.reorder(order);layout
}

impl crate::app::App {
    fn lyric_resident_preview(&self,ui:&mut egui::Ui) {
        ui.label(RichText::new("收起时显示：岛上歌词").size(11.5).color(ui.visuals().text_color()));
        let width=self.lyric_bar_width();
        let (stage,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),50.0),egui::Sense::hover());
        ui.painter().rect_filled(stage,10.0,Color32::from_rgb(19,25,36));
        let scale=((stage.width()-28.0)/width).min(1.0);
        let origin=Pos2::new(stage.center().x-width*scale/2.0,stage.top()+6.0);
        let body=Rect::from_min_size(origin,Vec2::new(width*scale,30.0*scale));
        if self.settings.island_shape==crate::model::IslandShape::Pill {ui.painter().rect_filled(body,15.0*scale,self.preview_fill());}
        else {crate::shape::fill(ui.painter(),body,8.0*scale,10.0*scale,12.0*scale,self.preview_fill(),1.0/ui.ctx().pixels_per_point());}
        if self.settings.skin.is_glass() {
            let rim=egui::Stroke::new(1.0_f32,Color32::from_white_alpha(60));
            if self.settings.island_shape==crate::model::IslandShape::Pill {ui.painter().rect_stroke(body,15.0*scale,rim,egui::StrokeKind::Inside);}
            else {ui.painter().add(egui::Shape::line(crate::shape::outline(body,8.0*scale,10.0*scale,12.0*scale),rim));}
        }
        let content_from=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
        let mut card=Card::new(ui,origin,stage).ghost();card.s=scale;
        let content_width=width-self.lyric_focus_space();
        if let Some((image,texture))=&self.music_artwork {
            let [w,h]=image.size;
            let uv_size=if w>h {egui::vec2(h as f32/w as f32,1.0)}else{egui::vec2(1.0,w as f32/h as f32)};
            let uv=Rect::from_center_size(Pos2::new(0.5,0.5),uv_size);
            card.painter.add(egui::epaint::RectShape::filled(card.rect(12.0,5.0,20.0,20.0),5.0*scale,Color32::WHITE).with_texture(texture.id(),uv));
        }else{
            card.fill(12.0,5.0,20.0,20.0,5.0,theme::CARD);
            paint::note(&card,22.0,15.0,13.0,theme::WHITE);
        }
        let text=card.fit(self.island_lyric_display_text(),11.0,W::Semibold,crate::app::LYRIC_BAR_W-80.0);
        card.centered_text(content_width-crate::app::LYRIC_BAR_W/2.0,15.0,&text,11.0,W::Semibold,theme::WHITE,H::Center);
        paint::playback_bars(&card,content_width-21.0,15.0,self.t,self.media.playing,theme::WHITE);
        if self.pomo.running && self.capsule_enabled(1) {
            crate::focus_bar::draw(&card,width-14.0-crate::focus_bar::LYRIC_WIDTH/2.0,15.0,self.t,&self.pomo,self.phase_secs(self.pomo.phase),true);
        }
        if let Some(palette)=self.skin_palette() {
            let end=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
            crate::skin_paint::recolor_layer(ui.ctx(),ui.layer_id(),&[(content_from,end)],&palette);
        }
        ui.label(RichText::new("普通胶囊排列（歌词暂时替换时不显示）").size(11.0).color(ui.visuals().weak_text_color()));
    }
    pub(crate) fn resident_order_settings(&mut self,ui:&mut egui::Ui){
        if self.island_lyrics_active() {self.lyric_resident_preview(ui);}
        let default=default_order(self.settings.show_battery&&self.resident_power.0.is_some()).to_vec();
        let mut order=if valid_order(&self.settings.resident_order){complete_order(&self.settings.resident_order)}else{default.clone()};
        ui.horizontal(|ui|{
            ui.label(RichText::new("拖动岛上的图标").color(ui.visuals().text_color()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if ui.small_button("恢复默认").clicked(){self.settings.resident_order.clear();order=default;}
            });
        });
        ui.add_space(12.0);
        let indices=self.quota_bar_indices();let count=indices.len();
        let mut layout=self.capsule_layout_for_preview(ui.ctx());
        layout.reorder(&order);
        for (position,extra) in self.quota_balance_layout(ui.ctx()){layout.balance_tail(position,extra);}
        if self.settings.clock_period!=0 { layout.widen_clock(40.0); }
        let (stage,_)=ui.allocate_exact_size(Vec2::new(ui.available_width(),100.0),egui::Sense::hover());
        ui.painter().rect_filled(stage,10.0,Color32::from_rgb(19,25,36));
        let scale=((stage.width()-28.0)/layout.width).min(1.0);
        let origin=Pos2::new(stage.center().x-layout.width*scale/2.0,stage.top());
        // 预览跟随当前的皮肤与形状。
        let pill=self.settings.island_shape==crate::model::IslandShape::Pill;
        let origin=if pill{origin+Vec2::new(0.0,4.0)}else{origin};
        let body=Rect::from_min_size(origin,Vec2::new(layout.width*scale,30.0*scale));
        let fill=self.preview_fill();
        if pill {ui.painter().rect_filled(body,15.0*scale,fill);}
        else {crate::shape::fill(ui.painter(),body,8.0*scale,10.0*scale,12.0*scale,fill,1.0/ui.ctx().pixels_per_point());}
        if self.settings.skin.is_glass() {
            let rim=egui::Stroke::new(1.0_f32,Color32::from_white_alpha(60));
            if pill {ui.painter().rect_stroke(body,15.0*scale,rim,egui::StrokeKind::Inside);}
            else {
                let edge=Rect::from_min_max(body.min+Vec2::new(0.5,0.0),body.max-Vec2::new(0.5,0.5));
                ui.painter().add(egui::Shape::line(crate::shape::outline(edge,8.0*scale,10.0*scale,12.0*scale-0.5),rim));
            }
        }
        let content_from=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
        let mut card=Card::new(ui,origin,stage);card.s=scale;
        let cx=layout.width/2.0;
        let pointer=ui.ctx().pointer_interact_pos();
        let dragged=egui::DragAndDrop::payload::<usize>(ui.ctx());
        let mut movement=None;
        let label=|slot|match slot{CLOCK=>"时间",BATTERY=>"电量",QUOTA=>"AI 额度",AGENT=>"Agent",MUSIC=>"音乐",FOCUS=>"专注",WEATHER=>"天气",CLIPBOARD=>"剪贴板",TODO=>"待办",MAIL=>"邮箱",CALENDAR=>"日程",MONITOR=>"系统监控",_=>"吉祥物"};
        for (index,&slot) in order.iter().enumerate(){
            let Some(position)=layout.x[slot] else {continue;};
            let x=cx+position;
            let width=match slot{AGENT|MONITOR=>layout.widths[slot],CLOCK=>if self.settings.clock_period!=0 {84.0}else{44.0},BATTERY=>20.0,PET=>32.0,MUSIC=>14.0,FOCUS=>crate::focus_bar::WIDTH,WEATHER=>64.0,CLIPBOARD|TODO|MAIL|CALENDAR=>38.0,_=>{
                let left=layout.quota_slots().filter_map(|i|layout.x[i].map(|x|x-layout.widths[i]/2.0)).fold(f32::INFINITY,f32::min);
                let right=layout.quota_slots().filter_map(|i|layout.x[i].map(|x|x+layout.widths[i]/2.0)).fold(f32::NEG_INFINITY,f32::max);
                right-left
            }};
            let left=if slot==QUOTA{cx+layout.quota_slots().filter_map(|i|layout.x[i].map(|x|x-layout.widths[i]/2.0)).fold(f32::INFINITY,f32::min)}else{x-width/2.0};
            let rect=card.rect(left-5.0,1.0,width+10.0,28.0);
            let response=ui.interact(rect,ui.id().with(("resident-icon",slot)),egui::Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
            response.dnd_set_drag_payload(slot);
            if let (Some(from),Some(point))=(dragged.as_deref(),pointer){
                if *from!=slot && rect.contains(point) && ui.input(|i|i.pointer.primary_down()){movement=Some((*from,index));}
            }
            let is_dragged=dragged.as_deref()==Some(&slot);
            let dx=if is_dragged{pointer.map(|p|((p.x-origin.x)/scale-(left+width/2.0)).clamp(-left,layout.width-left-width)).unwrap_or(0.0)}else{0.0};
            if is_dragged {ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);}
            else if response.hovered(){ui.painter().rect_filled(rect,6.0,Color32::from_white_alpha(18));}
            let active=match slot{BATTERY=>self.settings.show_battery&&self.resident_power.0.is_some(),PET=>self.settings.pet,QUOTA=>count>0,MUSIC=>self.media.is_music(),FOCUS=>self.pomo.running,_=>true};
            let c=card.shifted(dx,0.0).faded(if active||is_dragged{1.0}else{0.38}).ghost();
            match slot{
                BATTERY=>paint::battery(&c,x,15.0,self.resident_power.0,self.resident_power.1,1.0),
                CLOCK=>{let text=self.settings.clock_text(&chrono::Local::now().format("%H:%M:%S").to_string());paint::clock(&c,x-paint::clock_width(&c,&text,theme::size::PILL_CLOCK)/2.0,21.0,&text,theme::size::PILL_CLOCK);},
                PET=>self.pet.draw(&c,x,self.t,self.skin_palette()),
                MUSIC=>paint::playback_bars(&c,x,15.0,self.t,self.media.playing,theme::WHITE),
                FOCUS=>crate::focus_bar::draw(&c,x,15.0,self.t,&self.pomo,self.phase_secs(self.pomo.phase),false),
                QUOTA if count>0=>self.draw_quota_bar(&c,cx,&layout),
                AGENT=>self.draw_ai_chip(&c,x,15.0,width,false),
                WEATHER=>self.weather.draw_resident(&c,x,self.settings.weather_fahrenheit),
                CLIPBOARD|TODO|MAIL|CALENDAR|MONITOR=>{
                    let page=match slot {CLIPBOARD=>3,TODO=>4,MAIL=>5,MONITOR=>9,_=>7};
                    let today=chrono::Local::now().format("%Y-%m-%d").to_string();
                    self.draw_capsule_feature(&c,page,x,15.0,&today,width);
                },
                _=>{c.ring(x,15.0,10.0,2.0,theme::GRAY);c.text(x,18.0,"AI",7.0,W::Bold,theme::WHITE,H::Center);},
            }
            if is_dragged {ui.painter().line_segment([Pos2::new(rect.center().x,body.bottom()+6.0),Pos2::new(rect.center().x,body.bottom()+12.0)],egui::Stroke::new(2.0,theme::BLUE));}
            response.on_hover_text(label(slot));
        }
        if let Some(x)=layout.x[BELL] {
            paint::bell(&card,cx+x,15.0,theme::WHITE,0.0);
            card.circle(cx+x+4.0,10.0,2.0,Color32::from_rgb(255,72,72));
        }
        if let Some(palette)=self.skin_palette() {
            let end=crate::skin_paint::shape_count(ui.ctx(),ui.layer_id());
            crate::skin_paint::recolor_layer(ui.ctx(),ui.layer_id(),&[(content_from,end)],&palette);
        }
        if let Some((slot,index))=movement{
            if let Some(from)=order.iter().position(|value|*value==slot){order.remove(from);order.insert(index,slot);self.settings.resident_order=order;}
        }
        ui.painter().text(Pos2::new(stage.center().x,stage.bottom()-22.0),egui::Align2::CENTER_CENTER,"按住图标左右拖动",theme::font(11.0,W::Regular),Color32::from_gray(160));
        ui.add_space(8.0);
        ui.label(RichText::new("未显示的组件会保留排列位置。").size(11.0).color(ui.visuals().weak_text_color()));
    }
}
