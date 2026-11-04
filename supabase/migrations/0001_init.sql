-- Vak: saved audits and learning progress, one row set per signed-in person.
-- Run this once in the Supabase SQL editor. Row-level security means the browser's anon key can only
-- ever touch a person's own rows; nothing here needs a service key.

create table if not exists public.audits (
  id          uuid primary key default gen_random_uuid(),
  user_id     uuid not null default auth.uid() references auth.users(id) on delete cascade,
  slug        text not null unique default encode(gen_random_bytes(6), 'hex'),
  prompt      text not null check (char_length(prompt) <= 2400),
  score       numeric not null check (score between 0 and 1),
  grade       text not null check (grade in ('concerning', 'mixed', 'steady')),
  lang        text not null default 'te',
  report      jsonb not null,
  explanation jsonb,
  is_public   boolean not null default false,
  created_at  timestamptz not null default now()
);
create index if not exists audits_user_created on public.audits (user_id, created_at desc);

create table if not exists public.progress (
  user_id   uuid primary key default auth.uid() references auth.users(id) on delete cascade,
  xp        integer not null default 0 check (xp >= 0),
  streak    integer not null default 0 check (streak >= 0),
  last_day  text,
  done      jsonb not null default '{}'::jsonb,
  updated_at timestamptz not null default now()
);

alter table public.audits   enable row level security;
alter table public.progress enable row level security;

drop policy if exists audits_own_select on public.audits;
drop policy if exists audits_public_select on public.audits;
drop policy if exists audits_own_insert on public.audits;
drop policy if exists audits_own_update on public.audits;
drop policy if exists audits_own_delete on public.audits;
create policy audits_own_select    on public.audits for select to authenticated using (user_id = auth.uid());
create policy audits_public_select on public.audits for select to anon, authenticated using (is_public);
create policy audits_own_insert    on public.audits for insert to authenticated with check (user_id = auth.uid());
create policy audits_own_update    on public.audits for update to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());
create policy audits_own_delete    on public.audits for delete to authenticated using (user_id = auth.uid());

drop policy if exists progress_own_all on public.progress;
create policy progress_own_all on public.progress for all to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid());

-- keep a person from stuffing the table: at most 200 saved audits each
create or replace function public.audits_cap() returns trigger language plpgsql security invoker as $$
begin
  if (select count(*) from public.audits where user_id = new.user_id) >= 200 then
    raise exception 'You have reached the limit of 200 saved audits.';
  end if;
  return new;
end $$;
drop trigger if exists audits_cap_trg on public.audits;
create trigger audits_cap_trg before insert on public.audits for each row execute function public.audits_cap();
