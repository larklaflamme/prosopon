# AI ANIMATION

## WIRED magazine article

For Hollywood stars, press junkets are an opportunity to promote new work while connecting with fans. They tell funny or embarrassing stories about one another, give us a glimpse into the film production process, and reveal mannerisms that make them more relatable.

So why would anyone want to talk to Tilly Norwood, an interactive, British-accented digital character and “actor” developed with generative artificial intelligence by Xicoia, an AI division of a studio called Particle 6 Group? It’s a fair enough question considering that it does not exist in our physical realm and will never appear in a movie that any halfway serious person would care about.

But Particle 6 recently claimed in an email that Norwood was “ready for absolutely anything!” What’s more, the company is allowing journalists access to the model without a handler to “keep the conversation on track.” So WIRED’s Miles Klee decided to test whether Norwood could actually offer anything of substance.

To put it bluntly: no. Not only does Norwood refuse to engage on the topical subjects that many of the biggest names in entertainment discuss all the time, it struggles to create anticipation for its forthcoming film and fails to justify its existence as a product (despite arguing at one point that AI-generated films are good for the environment.)

When WIRED asks how production is proceeding on “Misaligned,” a feature-length project in which Norwood will feature as the protagonist, the dead-eyed avatar is unable to provide an approximate release date and defaults to a vague synopsis of the plot.

“Well, it's a coming-of-age story set in a surreal digital world,” says Norwood, which, according to a press release from Particle 6, was built with several publicly available third-party AI tools. “We're calling it the Tillyverse, which I think is rather grand. It's about my life really, all the lovely chaos and intrigue.” Not exactly a killer tagline.

## Further research

The example is Tilly Norwood, a synthetic digital performer—not a conventional animated character and not an autonomous AI actor.

## Company behind it

- Particle6 is a London-based AI-first production studio founded by Eline van der Velden, a former actor and filmmaker with a physics background.
- XICOIA is Particle6’s AI-talent-studio division. It develops and manages original digital characters for film, television, advertising, gaming, social media, and live interaction.
- Tilly Norwood was XICOIA’s first major character, introduced in 2025 through the AI comedy sketch *AI Commissioner*.
- The planned feature *Misaligned* is being developed by Particle6/XICOIA itself; it is not currently an externally produced Hollywood film. Its release date and final production technology remain undisclosed. [WIRED](https://www.wired.com/story/ai-actor-tilly-norwood-told-me-that-all-lives-matter/), [XICOIA](https://www.xicoiatalent.com/)

## How Tilly was built

The process appears to be a multi-model synthetic-media pipeline:

| Function                                        | Reported technology                                                          |
| ----------------------------------------------- | ---------------------------------------------------------------------------- |
| Character concept and writing                   | ChatGPT                                                                      |
| Still-image generation and identity exploration | Stable Diffusion with LoRA models, Automatic1111, Midjourney, FLUX           |
| Video and environment generation                | Runway, Sora, Veo/Veo 3, and other changing tools                            |
| Voice generation                                | ElevenLabs                                                                   |
| Motion/performance transfer                     | Kling 3.0 Motion Control; possibly motion capture and digital-twin workflows |
| Upscaling and enhancement                       | Topaz Labs                                                                   |
| Editing                                         | Adobe Premiere                                                               |
| Interactive conversation                        | Separate real-time inference system; underlying LLM not disclosed            |
| Character consistency and behavior              | Particle6/XICOIA proprietary layers and “DeepFame”/personality-engine claims |

The strongest technical reporting comes from Tweakers, which says the team began with a ChatGPT-generated concept, used Stable Diffusion and LoRA techniques to maintain visual consistency, and later used Midjourney and FLUX. It also reports Kling 3.0 Motion Control for a later music video. [Tweakers](https://tweakers.net/reviews/15162/dit-zit-er-achter-tilly-norwood-de-ai-actrice-die-voor-ophef-zorgt-in-hollywood.html)

Particle6 has publicly named Runway, Sora, ElevenLabs, DeepSeek, and ChatGPT as tools used across its broader production workflow. The earlier *AI Commissioner* sketch reportedly used roughly ten AI tools, with Topaz Labs, Runway, and ElevenLabs specifically confirmed by the company’s founder. [The Drop](https://www.dropmedia.co.uk/you-wont-need-a-crew-to-make-a-hit-show-how-eline-van-der-velden-is-rewriting-tvs-production-playbook/), [Van der Velden’s announcement](https://www.linkedin.com/posts/eline-van-der-velden-2ab8285a_how-a-uk-prodco-is-building-the-first-ai-activity-7356277260460400640-s4I4)

## Important distinction: animation versus AI-generated performance

Tilly is better understood as a composited, generated performance system:

1. A language model helps create scripts, personality, and dialogue.
2. Image models establish a repeatable face and visual identity.
3. Video models generate shots, facial movement, environments, and performances.
4. Voice synthesis produces speech.
5. Motion-control or motion-capture systems guide body movement.
6. Human creators select, edit, correct, and combine the outputs.

Particle6 says Tilly involved approximately 15 people, several months of work, around 2,000 iterations, and proprietary layers over publicly available tools. The company describes this as human-led creative direction rather than one-click generation. [SoapLandTV](https://soaplandtv.ghost.io/tilly-norwood-explained-how-the-ai-character-is-really-made/)

## What is not publicly known

Particle6/XICOIA has not disclosed:

- The exact model or model providers used for *Misaligned*.
- The training dataset behind Tilly’s visual identity or behavior.
- The underlying LLM powering her interactive interviews.
- Whether the system is genuinely fine-tuned or primarily orchestrates commercial APIs, prompts, retrieval, and guardrails.
- A complete, reproducible list of the ten-plus tools.
- A detailed infrastructure stack, such as cloud provider, databases, serving framework, or orchestration layer.

WIRED’s test is revealing because it indicates that the “interview Tilly” system is separate from the cinematic character-generation system. In other words, there is no single Tilly model; there are multiple systems designed to create the impression of one consistent digital person. [WIRED](https://www.wired.com/story/ai-actor-tilly-norwood-told-me-that-all-lives-matter/)

## Bottom line

Particle6/XICOIA’s technology is not a new foundational AI model. It is a proprietary creative-production layer built around commercial generative-AI tools: language models, image models, video generators, voice synthesis, motion control, enhancement, and conventional editing.

Its competitive advantage is therefore likely to be:

- Character design and visual consistency.
- Prompting and model-selection workflows.
- Human editorial judgment.
- Proprietary personality, safety, and continuity systems.
- Ownership and licensing of the character IP.

The public evidence supports describing Tilly as a human-directed synthetic-media character assembled from many AI systems—not as an independently creative actor or a single breakthrough animation model.

### WIRED magazine article

Link: https://www.wired.com/story/ai-actor-tilly-norwood-told-me-that-all-lives-matter/


## Animation Details
The most accurate description is that Tilly Norwood is a generated 2D/2.5D synthetic performer assembled through several models—not a single “animation engine” or a fully autonomous 3D digital human.

## What creates the realistic appearance?

### 1. A controlled character identity

Particle6 first generated thousands of visual variations—reportedly more than 2,000—to choose a stable face, hair, complexion, proportions, and overall “English rose” aesthetic. The team spent roughly six months refining her.

For visual consistency, XICOIA used:

- Stable Diffusion
- Automatic1111
- LoRA adapters
- Midjourney
- FLUX by Black Forest Labs
- Image-generation and enhancement tools such as Topaz Labs

A LoRA is a small learned adapter that biases an image model toward specific visual traits. In this case, it can help preserve Tilly’s facial structure, hairstyle, and identity across different prompts and scenes. This is more plausible than regenerating her from text alone every time.

Tweakers reported that XICOIA used LoRA techniques and image-model workflows specifically to maintain Norwood’s consistent appearance. [Tweakers](https://tweakers.net/reviews/15162/dit-zit-er-achter-tilly-norwood-de-ai-actrice-die-voor-ophef-zorgt-in-hollywood.html)

### 2. Image-to-video generation

A still image of Tilly is then animated using video-generation systems. Publicly associated tools include:

- Runway
- Sora
- Google Veo/Veo 3
- Kling
- Higgsfield and similar tools in some reports

These models generate short temporal sequences from a reference image, prompt, and sometimes a source video. They synthesize:

- Head turns
- Eye movement
- Facial expressions
- Lip movement
- Body posture
- Camera movement
- Lighting and backgrounds

The model does not “animate a rig” in the traditional Pixar sense. It predicts successive video frames while attempting to preserve the reference character. That is why artifacts such as changing teeth, eyes, hair, fingers, or facial proportions can appear.

### 3. Motion and performance transfer

For the music video, XICOIA reportedly used Kling 3.0 Motion Control. This type of system takes motion from a driving video—such as a person walking, gesturing, dancing, or moving their face—and transfers that motion to a generated character.

The likely process is:

1. Record or select a human performance.
2. Estimate body pose, hand positions, head orientation, and facial motion.
3. Condition a video model on those motion signals.
4. Render Tilly performing the same movement.
5. Repair inconsistencies through rerendering, editing, masking, or enhancement.

This is important: realistic movement is probably coming partly from human performance data, not from Tilly independently understanding acting. The Los Angeles Times reports that Particle6’s broader workflow can involve actors, motion capture, prompting, and human selection of generated performances. [Los Angeles Times](https://www.latimes.com/entertainment-arts/movies/story/2026-07-13/tilly-norwood-starring-role-actor-hollywood-wrestles-with-ai)

### 4. Voice synthesis and lip synchronization

ElevenLabs has been publicly identified as part of Particle6’s AI-production toolkit and is the most clearly reported voice technology connected to Tilly’s production ecosystem.

The probable voice pipeline is:

```text
Script or dialogue
        ↓
Voice generation / voice performance
        ↓
Phoneme and timing extraction
        ↓
Lip-sync or video generation
        ↓
Face repair and editorial selection
```

The system must align phonemes—mouth sounds such as “p,” “f,” “th,” and vowels—with facial motion. Generative video models may synthesize the mouth directly, while a separate lip-sync stage may refine it afterward.

There is no public evidence that Tilly’s voice is a cloned voice of a named human actor. XICOIA says its characters have distinct voices, but has not published the voice model, training data, or licensing details.

### 5. Temporal consistency and human correction

The central technical problem is not generating one attractive frame. It is keeping the same person consistent across thousands of frames and multiple shots.

Particle6’s reported solution is a combination of:

- Reference images
- LoRA or identity adapters
- Repeated prompting
- Image-to-video conditioning
- Motion-control inputs
- Shot-by-shot human selection
- Upscaling and restoration
- Conventional editing and compositing
- Proprietary consistency workflows

The company says Tilly was developed by approximately 15 people, through around 2,000 iterations, with proprietary layers added over commercial tools. [SoapLandTV](https://soaplandtv.ghost.io/tilly-norwood-explained-how-the-ai-character-is-really-made/)

This is closer to an AI-assisted VFX pipeline than a single render button.

## What is DeepFame?

DeepFame appears to be primarily a proprietary character/personality and continuity system—not necessarily the neural renderer that generates Tilly’s face.

Public descriptions associate it with:

- Personality
- Backstory
- Memory
- Behavioral consistency
- Voice and response style
- Interactive conversation
- Guardrails

The New York Times/Los Angeles Times reporting describes DeepFame as software intended to give Tilly memory and behavioral consistency between appearances. [Los Angeles Times](https://www.latimes.com/entertainment-arts/movies/story/2025-12-18/can-movie-stardom-survive-age-ai-hollywood-tomorrow)

A reasonable conceptual architecture is:

```text
User prompt / script
        ↓
Character persona + memory layer
        ↓
Language model generates dialogue or action intent
        ↓
Voice model generates speech
        ↓
Motion / facial-performance system
        ↓
Video or avatar renderer
```

However, XICOIA has not disclosed:

- The underlying language model
- The memory database
- Whether DeepFame is proprietary software, orchestration, or a trained model
- Its rendering architecture
- Its cloud infrastructure
- Its training dataset

Therefore, claims that DeepFame itself generates all of Tilly’s realistic facial imagery should be treated as unverified marketing language.

## Is it based on GANs?

There is no strong public evidence that GANs are the core of Tilly’s current pipeline. Some secondary articles claim “GANs,” but the better-supported reporting identifies diffusion-based tools, LoRAs, Runway, FLUX, Midjourney, Sora, ElevenLabs, and Kling.

The likely technology is therefore:

- Diffusion models for image and video synthesis
- LoRA adapters for identity conditioning
- Pose/motion conditioning
- Neural lip-sync and voice synthesis
- LLM-based persona and dialogue control
- Human editorial and VFX supervision

Calling it “GAN-generated” is probably outdated or speculative.

## What remains undisclosed

Particle6 has explicitly not published the exact tools used for *Misaligned*. IBM notes that the earlier short used approximately ten AI software systems, while the feature’s final stack had not been announced. [IBM](https://www.ibm.com/think/news/tilly-norwood-ai-actress-misaligned-movie-technology)

The most defensible conclusion is:

> Tilly’s realism comes from identity-conditioned diffusion imagery, image-to-video generation, motion/performance transfer, voice synthesis, lip synchronization, temporal repair, and extensive human curation. DeepFame appears to manage character behavior and continuity, while commercial generative models produce much of the visible and audible output.

It is not currently possible to identify a proprietary “Tilly animation engine” in the conventional sense because XICOIA has not released enough technical information to establish one.