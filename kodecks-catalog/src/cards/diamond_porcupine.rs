use crate::card_def;
use kodecks::prelude::*;

card_def!(
    CardDef,
    "diam",
    "Diamond Porcupine",
    color: Color::RUBY,
    cost: 2,
    card_type: CardType::Creature,
    power: 100,
);

impl Effect for CardDef {
    fn event_filter(&self) -> EventFilter {
        EventFilter::DEALT_DAMAGE
    }

    fn trigger(&mut self, id: EffectId, ctx: &mut EffectTriggerContext) -> Result<()> {
        if id == "main" {
            ctx.push_stack("main", |ctx, _| {
                let card = ctx.source();
                let target = card.zone().player;
                let commands = vec![ActionCommand::GenerateShards {
                    player: target,
                    source: ctx.source().id(),
                    color: card.computed().color,
                    amount: 1,
                }];
                Ok(EffectReport::default().with_commands(commands))
            });
        }
        Ok(())
    }

    fn activate(&mut self, _event: CardEvent, ctx: &mut EffectActivateContext) -> Result<()> {
        ctx.trigger_stack("main");
        Ok(())
    }
}