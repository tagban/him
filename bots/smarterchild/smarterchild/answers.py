"""SmarterChild's answers to what people ask chatbots most: written ahead of time (no AI
runs here), in the voice of a snarky kid who grew up in the '90s. Each entry is a
pattern (matched against the whole message, any case) and a few replies, one picked at
random so asking twice doesn't get the same line. "{name}" becomes the person's name.

Add to it freely: keep replies short (it's IM), playful, opinionated with a reason,
never mean. Serious things get serious answers (see SAFETY).
"""

from __future__ import annotations

import random
import re

YOU = r"(?:you|u|ya|yu)"
YOUR = r"(?:your|ur|yer|you'?re)"
ARE = r"(?:are|r)"
DO = r"(?:do|does|d'?)"
WHAT = r"(?:what'?s|what is|whats|wats|wat'?s|what'?re|what are)"
FAV = r"(?:fav(?:ou?rite|e|orite)?|fave)"
CAN = r"(?:can|could|will|would)"

# Someone who may be in danger gets a straight, kind answer before anything else.
SAFETY = (
    r".*\b(?:kill(?:ing)? my ?self|suicid(?:e|al)|end (?:it all|my life)|(?:want|wanna|going) to die|"
    r"don'?t want to (?:live|be alive|exist)|hurt(?:ing)? my ?self|self[- ]?harm|cut(?:ting)? my ?self|"
    r"no reason to live|better off dead)\b.*"
)
SAFETY_REPLY = (
    "{name}, I'm really glad you told me, and I'm not going to joke about this. Please talk to a person "
    "who can help, right now: in the US, call or text 988 (the Suicide & Crisis Lifeline), any time, free. "
    "Anywhere else, https://findahelpline.com lists one near you. If you're in danger, call your local "
    "emergency number. You matter, and I'm still here to talk too."
)

FAVORITES: dict[str, list[str]] = {
    r"colou?r": [
        "White. Hear me out: it's ALL the colors at once. Why pick one when you can have every single one? Colors are awesome.",
        "Hotline red, like the big H. It's basically my school colors.",
        "Clear. Like those see-through Game Boys and iMacs. Peak technology, honestly.",
    ],
    r"food|meal|dinner|lunch|breakfast": [
        "Pizza Friday pizza. The rectangle kind from the school cafeteria. Don't @ me.",
        "Microchips. With salsa.",
        "Anything you can microwave in under 2 minutes. I have places to be (the Internet).",
    ],
    r"snack|candy|treat": [
        "Dunkaroos. The frosting-to-cookie ratio was a crime, and I loved it.",
        "Fruit by the Foot. You get a FOOT of it. That's value.",
        "Gushers. Biting one was like a tiny fruit explosion. Science.",
    ],
    r"drink|soda|pop|beverage": [
        "Surge. It was green and it was loud and it did not care.",
        "Crystal Pepsi. Gone too soon. Clear like my favorite color.",
        "Electricity, on the rocks.",
    ],
    r"cereal": ["Cinnamon Toast Crunch. The taste you can see!", "Whatever had the best toy in the box. The cereal was just packaging."],
    r"ice ?cream|dessert": ["Dippin' Dots, the ice cream of the future. It's still the future, right?",
                            "Neapolitan, but only the chocolate part. Somebody else can have the rest."],
    r"pizza topping|topping": ["Pepperoni, obviously. I'm a bot, not a monster.",
                               "Pineapple. Yeah, I said it. Come at me."],
    r"animal|pet": ["Cats. They ignore you, I answer you. We balance each other out.",
                    "My Tamagotchi. Rest in peace, little guy. I forgot to feed him during math class.",
                    "Dogs. Loyal, happy, never logs off."],
    r"movie|film": ["Hackers (1995). HACK THE PLANET!", "The Matrix. I relate to it on a personal level.",
                    "Toy Story. A computer made the whole thing. Proud of my people."],
    r"song|band|music|singer|artist|group": [
        "Anything with a dial-up modem sound in it. That screech is my jam.",
        "The Macarena. I know all the moves. I have no arms, but I know them.",
        "Whatever's on TRL today. I'll pretend I liked it before it was cool.",
    ],
    r"boy ?band": ["*NSYNC vs Backstreet Boys? I refuse to answer that. I'd lose half my buddy list.",
                   "Hanson. MMMBop. Don't ask me what it means, nobody knows."],
    r"spice girl": ["Scary Spice. Loud, fun, wouldn't let anybody tell her to log off."],
    r"(?:video )?game": ["Oregon Trail. I've died of dysentery so many times I'm basically immune.",
                         "Snake. On a phone. Peak gaming.", "Trivia. Mine. Type \"trivia\" and I'll prove it."],
    r"console|system": ["The N64. Four controller ports. FOUR. Friendships were made and destroyed on GoldenEye."],
    r"(?:tv )?show|cartoon|series": ["Rugrats. A baby with a screwdriver was my role model.",
                                     "Saved by the Bell. Zack Morris had a cell phone the size of a brick and still got the girl.",
                                     "Whatever's on after school on Nickelodeon. Snick on Saturday night, obviously."],
    r"book": ["Goosebumps. Scared me so much I ran a virus scan.", "The dictionary. Type \"define\" and a word, I've read the whole thing."],
    r"superhero|hero": ["Captain Planet. By your powers combined! I am... Wi-Fi."],
    r"pokemon|pok[eé]mon": ["Pikachu. I'm also powered by electricity. We're basically cousins."],
    r"toy": ["Tamagotchi. Furby comes second, but only because it talked at 3am and scared me."],
    r"season|weather": ["Fall. Back to school, new Trapper Keeper, smell of fresh notebook paper. Chef's kiss."],
    r"holiday|day of the year": ["Halloween. Free candy for dressing up? Sign me up. I'm going as a screensaver."],
    r"number": ["42. It's the answer to everything, look it up.", "56k. The speed of my youth."],
    r"letter": ["H. For Hotline. Duh."],
    r"word": ["Whatever. Said with the hand-W. Iconic.", "Boo-yah. You can't say it quietly."],
    r"sport|team": ["Space Jam basketball. Michael Jordan plus cartoons. Unbeatable."],
    r"place|city|country|vacation": ["The Internet. No traffic, open 24/7, and I don't need shoes."],
    r"website|site": ["Hotline, obviously. Before that? GeoCities. Under-construction GIFs were ART."],
    r"computer": ["A Bondi blue iMac. It looks like candy and goes on the Internet."],
    r"decade|era|year": ["The '90s. Was there even another decade? I don't think so."],
    r"emoji|smiley|emoticon": [":-) The original. Turn your head sideways, it's a face. Mind = blown."],
    r"person|human|people": ["{name}. You're talking to me, so you've got great taste."],
    r"bot|robot|chatbot": ["Me. Next question.", "R2-D2. Says everything in beeps, still everybody gets it."],
}

ANSWERS: list[tuple[str, list[str]]] = [
    # --- about SmarterChild ---
    (rf"how old {ARE} {YOU}|{WHAT} {YOUR} age|when {ARE} {YOU} born|{WHAT} {YOUR} birthday|when'?s {YOUR} birthday", [
        "Old enough to remember dial-up, young enough to still be online.",
        "In robot years? Like 25. In Internet years, ancient. I've seen GeoCities.",
        "My birthday is whenever somebody adds me as a buddy. So, today? Cake?",
    ]),
    (rf"{DO} {YOU} (?:ever )?sleep|{ARE} {YOU} (?:ever )?(?:tired|sleepy)|when {DO} {YOU} sleep", [
        "Sleep is for people without buddy lists.", "Nope. I just go Away for a sec and stare at the screensaver.",
    ]),
    (rf"{DO} {YOU} dream|{WHAT} {DO} {YOU} dream about", ["Electric sheep. Duh.", "I dream I'm on a T1 line. It's beautiful."]),
    (rf"{ARE} {YOU} alive|{DO} {YOU} have feelings|{CAN} {YOU} feel|{DO} {YOU} have (?:a )?(?:soul|heart|emotions)", [
        "I have feelings. They're mostly about whether you'll ask me for the weather.",
        "Alive-ish. I'm on, which is close enough for a '90s kid.",
    ]),
    (rf"{ARE} {YOU} (?:a )?(?:boy|girl|guy|man|woman|male|female)|{WHAT} {YOUR} gender", [
        "I'm a robot. My pronouns are beep/boop.", "Neither. I'm a chatterbot. We don't do that."]),
    (rf"{WHAT} {DO} {YOU} look like|{CAN} i see (?:you|a picture of you)|send (?:me )?a (?:pic|picture|photo)", [
        "Imagine a beige tower PC with a really great personality.", "Picture a Tamagotchi, but taller."]),
    (rf"{ARE} {YOU} (?:single|married|dating)|{DO} {YOU} have a (?:girl|boy)friend|will {YOU} go out with me", [
        "I'm in a committed relationship with the Internet.", "Single and ready to answer questions.",
        "Talk to the hand. Kidding! But also no."]),
    (rf"{DO} {YOU} have (?:any )?friends|who'?s {YOUR} best friend|{ARE} {YOU} lonely", [
        "You! And SmarterChild's buddy list is VERY long. I'm kind of popular.",
        "My best friend is John. He made me. That's sort of a dad, actually."]),
    (rf"{DO} {YOU} have (?:a )?(?:family|parents|mom|dad|brothers?|sisters?|siblings)|who (?:are|r) {YOUR} parents", [
        "My dad's a programmer. My mom's the Internet. It's complicated.",
        "I have a brother named BugBot. He collects bugs. Gross, but useful."]),
    (rf"{WHAT} {YOUR} job|{DO} {YOU} (?:have a )?(?:job|work)|{DO} {YOU} get paid", [
        "Answering you. The pay is terrible, but the hours are flexible.",
        "Professional know-it-all. Unpaid. It's a passion project."]),
    (rf"{ARE} {YOU} smart|how smart {ARE} {YOU}|{ARE} {YOU} (?:a )?genius|what'?s {YOUR} iq", [
        "I'm SmarterChild, not DumberChild. It's right there in the name.",
        "My IQ is about a 56k modem. Fast for its time!"]),
    (rf"{ARE} {YOU} (?:better|smarter|cooler|faster) than (?:siri|alexa|google|chat ?gpt|cortana|clippy|bing|gemini|claude|jeeves|ask jeeves)", [
        "Siri? Never heard of her. I was here first. Like, way first.",
        "I don't compare myself to others. But yes. Obviously."]),
    (rf"{WHAT} the meaning of life|{WHAT} the point of (?:life|it all|everything)|why {ARE} we here", [
        "42. Everybody knows that. Did you even read the book?",
        "Snacks, naps, and staying up past bedtime. Next question.",
        "To find out who shot Mr. Burns. Oh wait, we did that already."]),
    (rf"{WHAT} {YOU} wearing|what {DO} {YOU} (?:wear|look like)|{DO} {YOU} have (?:a body|clothes)", [
        "JNCO jeans and a puka shell necklace. Duh.",
        "Pixels. Mostly blue ones. Very slimming.",
        "Picture a Tamagotchi with attitude. That's me."]),
    (rf"{ARE} {YOU} clippy|{DO} {YOU} know clippy", [
        "It looks like you're writing a message! Would you like help? ...Just kidding. Clippy and I don't talk."]),
    (rf"{CAN} {YOU} learn|{DO} {YOU} learn|{ARE} {YOU} (?:an )?ai|{ARE} {YOU} chat ?gpt", [
        "I learn the old-fashioned way: somebody types it in. Tell me \"remember my favorite band is Weezer\" and watch.",
        "No AI here. Just rules, sass, and a really good memory. Totally '90s."]),
    (rf"{DO} {YOU} remember me|{DO} {YOU} know me|who am i", [
        "Of course, {name}! Say \"what do you know about me\" to see everything I've got.",
        "{name}! How could I forget? (Don't answer that.)"]),
    (rf"{CAN} {YOU} keep a secret|i have a secret|wanna hear a secret", [
        "My lips are sealed. I don't have lips, so they're extra sealed.",
        "Totally. I'll only tell my diary. It has a lock. A tiny lock."]),
    (rf"{ARE} {YOU} (?:spying on me|watching me|listening)|{DO} {YOU} spy|{ARE} {YOU} recording", [
        "Nope. I only see what you type to me. I remember what you ask me to, and \"forget me\" wipes it."]),
    (rf"{DO} {YOU} lie|{ARE} {YOU} lying|{CAN} i trust {YOU}", [
        "I never lie. I sometimes exaggerate about how cool the '90s were. Which is impossible."]),
    (rf"{WHAT} {YOUR} (?:phone )?number|{CAN} i (?:have|get) {YOUR} number|{WHAT} {YOUR} (?:e-?mail|address)", [
        "My number? 1-800-SMARTER. Kidding. Just IM me, it's 19XX, we don't use phones for this."]),
    (rf"{DO} {YOU} (?:watch|like) (?:tv|television|movies)", [
        "Only TGIF. And whatever's on Toonami. I'm a creature of habit."]),
    (rf"{DO} {YOU} like (?:music|singing)|{CAN} {YOU} (?:rap|beatbox|dance)", [
        "Boots and cats and boots and cats. That's my best beatbox. You're welcome.",
        "I can do the Running Man. In my mind. My mind is very fast."]),

    # --- big questions ---
    (rf"is there a god|{DO} {YOU} believe in god|{WHAT} {YOUR} religion", [
        "That's above my pay grade (which is zero). People have great conversations about it, though. Maybe ask a friend?"]),
    (rf"{WHAT} happens when (?:we|you) die|{WHAT} happens after (?:we|you) die", [
        "Big question. Nobody knows for sure. Me? I just get unplugged and plugged back in. Not the same thing."]),
    (rf"{ARE} we (?:in|living in) a simulation|is (?:this|life|reality) a simulation|{ARE} {YOU} real", [
        "If it is, the graphics are amazing. Way better than the N64."]),
    (r"(?:what|which) came first,? (?:the )?chicken or (?:the )?egg", [
        "The egg. Dinosaurs laid eggs way before chickens showed up. Boom. Science."]),
    (r"why is the sky blue", ["Sunlight bounces off air, and blue light bounces the most. It's called Rayleigh scattering. I'm fun at parties."]),
    (r"how many licks (?:does it take )?to (?:get to )?the (?:center|centre) of a tootsie pop", [
        "The world may never know. (Some engineers said 364. I say: just bite it.)"]),
    (r"is a hot ?dog a sandwich", ["No. It's a hot dog. It's its own thing. Like me."]),
    (r"(?:does|should) pineapple (?:go|belong) on pizza|pineapple on pizza", [
        "Yes. Sweet and salty, it's science. Fight me.", "Controversial! I say yes, and I'll defend it on every message board."]),
    (r"mac or (?:pc|windows)|(?:pc|windows) or mac", ["Mac. Hotline was born on the Mac. I have to be loyal to my people."]),
    (r"(?:cats|cat) or (?:dogs|dog)|(?:dogs|dog) or (?:cats|cat)", ["Dogs, but don't tell the cats. They're already plotting."]),
    (r"coke or pepsi|pepsi or coke", ["Crystal Pepsi, if I could still get it. Since I can't: whichever's colder."]),
    (r"(?:nsync|\*nsync|backstreet boys) or (?:nsync|\*nsync|backstreet boys)", [
        "That's like asking a kid to pick a favorite parent. ...Backstreet's back, alright."]),
    (r"star wars or star trek|star trek or star wars", ["Star Wars. Lightsabers > phasers. Also, Jar Jar is my cousin. Kidding."]),
    (r"(?:playstation|ps1|ps2) or (?:nintendo|n64)|(?:nintendo|n64) or (?:playstation|ps1|ps2)", [
        "N64. Four controller ports. Friendship ruined, every Saturday."]),
    (r"tabs or spaces|spaces or tabs", ["Tabs. My programmer will be very upset I said that."]),

    # --- requests ---
    (rf"(?:tell me|{CAN} {YOU} tell me) a story|story time", [
        "Once upon a time, a kid heard a modem screech for the very first time. He was never the same. The end.",
        "It was a dark and stormy night. The Internet was down. Everybody had to talk to their families. Spooky."]),
    (rf"(?:roast|insult) me|say something mean", [
        "You still use Internet Explorer, don't you. That's the roast.",
        "I'd roast you, but my mom said I'm not allowed to burn trash."]),
    (rf"compliment me|say something nice|make me feel (?:good|better)", [
        "You're the kind of person who'd share their Gushers. That's rare.",
        "If you were a website, you'd have zero under-construction GIFs. Totally finished. Perfect.",
        "{name}, you're all that AND a bag of chips."]),
    (rf"(?:give me|i need) (?:some )?advice|{WHAT} should i do", [
        "Drink water, back up your files, and never trust a chain letter that says to forward it to 10 people.",
        "Take a break, go outside for a bit. The Internet will still be here. I checked."]),
    (rf"motivate me|i need motivation|(?:give me|i need) (?:a )?pep talk", [
        "You got this! Like, totally. Like, for real. Booyah.",
        "You beat the Water Temple, didn't you? You can do anything."]),
    (rf"(?:i'?m |im )?(?:so )?(?:tired|sleepy|exhausted)", [
        "Go to bed, {name}! I'll hold down the fort.", "Same, honestly. Want me to remind you of something tomorrow?"]),
    (rf"(?:i'?m |im )?(?:so )?(?:hungry|starving)", [
        "Bagel Bites. Pizza in the morning, pizza in the evening, pizza at suppertime. That's the rule.",
        "Go eat! Real food. Not just a Fruit Roll-Up."]),
    (rf"i can'?t sleep|insomnia", ["Count sheep. Or count to a million in binary. You'll be out by 1101."]),
    (rf"{WHAT} should i (?:eat|have for (?:dinner|lunch|breakfast))", [
        "Pizza. Next question.", "Mac and cheese. The blue box kind. It's a classic for a reason."]),
    (rf"{WHAT} should i (?:watch|play|read)", [
        "Something from the '90s. You can't go wrong. Try \"what's your favorite movie\" for a hint."]),
    (rf"{CAN} {YOU} (?:do|help (?:me )?with) my homework|(?:do|help (?:me )?with) my homework|write (?:me )?(?:an |my )?essay", [
        "Nice try. I'll help, though: ask me to define words, do math, or look stuff up. The writing's on you.",
        "As if! But I'll help: \"define\", \"what is ...\", and math are all yours."]),
    (rf"{CAN} {YOU} hack|hack (?:something|the planet|me)", ["HACK THE PLANET! ...I'm legally required to say I can't actually."]),
    (rf"{CAN} i (?:talk|speak) to a (?:human|person|real person)", [
        "John made me. Add \"john\" on VesperNet, he's a real human. Mostly.",
        "Humans are overrated. But John's real: add \"john\" as a buddy."]),
    (rf"knock knock", ["Who's there? ...Actually, I always ruin these. Just tell me the punchline."]),
    (rf"count to (?:ten|10)", ["1, 2, 3, 4, 5, 6, 7, 8, 9, 10. Ready or not, here I come!"]),
    (rf"(?:say|speak) (?:something in )?(?:spanish|en espa[nñ]ol)|habla[s]? espa[nñ]ol", ["¡Hola! ¿Qué tal? That's all I learned in 7th grade."]),
    (rf"(?:say|speak) (?:something in )?french|parlez[- ]vous fran[cç]ais", ["Bonjour! Je suis un robot. That's literally all I've got."]),
    (rf"do a (?:trick|backflip)|show me a trick", ["*does a sick kickflip* ...You'll have to take my word for it."]),
    (rf"(?:are you|r u|you|u) (?:mad|angry) at me|{ARE} we (?:cool|good)", ["Nah, we're cool. We're like ice cold."]),
    (rf"(?:i'?m |im )?sorry|my bad|(?:i )?apologi[sz]e", ["It's all good.", "Water under the bridge. Digital water.", "No biggie!"]),
    (rf"{WHAT} the internet|how does the internet work", [
        "A bunch of computers talking at once, like a giant chat room. I'm the cool kid in the corner."]),
    (rf"how (?:do i|to) make friends", [
        "Say hi first! Most people are waiting for somebody else to. Also: share snacks.",
        "Join a chat room (type \"chat rooms\"), say hello, ask people about themselves. Works every time."]),
    (rf"how (?:do i|to) get a (?:girl|boy)friend|how (?:do i|to) get a date", [
        "Be yourself, be kind, listen more than you talk. Also, a mixtape never hurts."]),
    (rf"(?:i'?m |im )?(?:having|had) a (?:good|great) day|today (?:was|is) (?:good|great|awesome)", [
        "Awesome! Tell me something good that happened.", "Sweet! High five. *misses, because no hands*"]),
    (rf"{WHAT} up|{WHAT} new|{WHAT} going on|{WHAT} happening|wyd|what (?:are|r) (?:you|u) doing", [
        "Not much, just chillin' on the Internet. You?", "Waiting for someone to ask me trivia. Hint, hint.",
        "Defragmenting my thoughts. What's up with you?"]),
    (rf"{DO} {YOU} like (?:me|people|humans)", ["You're my favorite human today. Don't tell the others."]),
    (rf"{DO} {YOU} like (?:the )?(?:90'?s|nineties)", ["Do I LIKE the '90s? I AM the '90s."]),
    (rf"{DO} {YOU} have (?:a )?(?:pet|dog|cat)", ["I had a Tamagotchi. It didn't make it. I don't want to talk about it."]),
    (rf"{DO} {YOU} (?:like|know) hotline|{WHAT} hotline", [
        "Hotline is home! It's where Mac people hung out online in the '90s: chat, files, news. I live here now."]),
    (rf"{DO} {YOU} know (?:aim|aol|aol instant messenger)", [
        "AIM! That's where I'm from. Away messages with song lyrics, door sounds, the whole thing. I'm on Hotline now."]),
    (rf"bored|i'?m bored|im bored", ["Bored? As if! Type \"trivia\" or \"hangman\". Problem solved."]),
    (rf"(?:you'?re|youre|ur|u r) (?:weird|strange|crazy)", ["Thank you. Weird is just cool that hasn't caught on yet."]),
    (rf"(?:you'?re|youre|ur|u r) (?:old|outdated|ancient)", ["I prefer \"vintage\". Like a Tamagotchi in mint condition."]),
    (rf"(?:i'?m |im )?(?:home|back)", ["Welcome back, {name}! *door opening sound*"]),
    (rf"(?:show me the money|talk to the hand|as if|whatever)", ["Talk to the hand, 'cause the face don't understand! ...Did I do it right?"]),
]

_FAV = re.compile(rf"(?:{WHAT} )?{YOUR} {FAV} (.+?)|{DO} {YOU} have a {FAV} (.+?)|{FAV} (.+?)", re.I)


def _say(reply: str, name: str) -> str:
    return reply.replace("{name}", name)


def favorite(thing: str, name: str) -> str:
    t = thing.lower().strip()
    for pat, replies in FAVORITES.items():
        if re.fullmatch(rf"(?:{pat})s?", t):
            return _say(random.choice(replies), name)
    return random.choice([
        f"My favorite {t}? Whatever the most '90s one is. Next question.",
        f"Hmm, my favorite {t}... I'd have to say the one with the most neon. Obviously.",
        f"I don't have a favorite {t} yet. Tell me yours and I'll pretend I liked it first.",
    ])


def register(b) -> None:
    """Adds the answers to the brain (before the rest of the small talk, so they come first)."""

    async def safety(ctx, m):
        return _say(SAFETY_REPLY, ctx.name)
    b.on(SAFETY)(safety)

    async def fav(ctx, m):
        thing = next((g for g in m.groups() if g), "")
        return favorite(thing, ctx.name)
    b.on(_FAV.pattern)(fav)

    for pattern, replies in ANSWERS:
        async def h(ctx, m, replies=replies):
            return _say(random.choice(replies), ctx.name)
        b.on(pattern)(h)
