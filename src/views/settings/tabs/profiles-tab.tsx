import type { UseProfiles } from "../hooks/use-profiles";

export function ProfilesTab({ profiles }: { profiles: UseProfiles }) {
  return (
    <>
      <h2>Profiles</h2>
      <section>
        <p className="placeholder">
          Each profile keeps its own break cadence, hints, and overlay settings.
          Switching is instant. The active profile drives every other tab here, and
          appears in the tray under "Active profile".
        </p>
        {profiles.profileError && (
          <p className="profile-error">{profiles.profileError}</p>
        )}
        <div className="profile-list">
          {profiles.profiles.map((name, idx) => {
            const isActive = name === profiles.activeProfile;
            const draft = profiles.renameDrafts[name];
            const isRenaming = draft !== undefined;
            const isDeleteCandidate = profiles.deleteCandidate === name;
            const isResetCandidate = profiles.resetCandidate === name;
            const canDelete = !isActive && profiles.profiles.length > 1;
            const canMoveUp = idx > 0;
            const canMoveDown = idx < profiles.profiles.length - 1;
            return (
              <div
                key={name}
                className={`profile-row${isActive ? " active" : ""}`}
              >
                <span className="profile-reorder">
                  <button
                    type="button"
                    className="reorder-btn"
                    aria-label={`Move ${name} up`}
                    disabled={isRenaming || !canMoveUp}
                    onClick={() => profiles.move(name, -1)}
                  >
                    <span aria-hidden="true">▲</span>
                  </button>
                  <button
                    type="button"
                    className="reorder-btn"
                    aria-label={`Move ${name} down`}
                    disabled={isRenaming || !canMoveDown}
                    onClick={() => profiles.move(name, 1)}
                  >
                    <span aria-hidden="true">▼</span>
                  </button>
                </span>
                {isRenaming ? (
                  <input
                    type="text"
                    aria-label="Profile name"
                    value={draft}
                    // Autofocus is appropriate here: the field appears in
                    // response to a user clicking "rename", not on page
                    // load, so focusing it is expected, not disorienting.
                    // eslint-disable-next-line jsx-a11y/no-autofocus
                    autoFocus
                    onChange={(e) => profiles.setRenameDraft(name, e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") profiles.rename(name);
                      if (e.key === "Escape") profiles.setRenameDraft(name, null);
                    }}
                    onBlur={() => profiles.rename(name)}
                  />
                ) : (
                  <span className="profile-name">
                    {name}
                    {isActive && <span className="profile-badge">active</span>}
                  </span>
                )}
                <span className="profile-actions">
                  {!isRenaming && !isActive && (
                    <button
                      type="button"
                      className="icon-action"
                      aria-label={`Use profile ${name}`}
                      title="Use this profile"
                      onClick={() => profiles.switchTo(name)}
                    >
                      <span aria-hidden="true">○</span>
                    </button>
                  )}
                  {!isRenaming && (
                    <button
                      type="button"
                      className="icon-action"
                      aria-label={`Rename profile ${name}`}
                      title="Rename"
                      onClick={() => profiles.setRenameDraft(name, name)}
                    >
                      <span aria-hidden="true">✎</span>
                    </button>
                  )}
                  {!isRenaming && (
                    <button
                      type="button"
                      className="icon-action"
                      aria-label={`Duplicate profile ${name}`}
                      title="Duplicate"
                      onClick={() => profiles.duplicate(name)}
                    >
                      <span aria-hidden="true">⧉</span>
                    </button>
                  )}
                  {!isRenaming &&
                    (isResetCandidate ? (
                      <button
                        type="button"
                        className="link danger"
                        onClick={() => profiles.confirmReset(name)}
                      >
                        Confirm reset
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="icon-action icon-accent"
                        aria-label={`Reset profile ${name} to defaults`}
                        title="Reset to defaults"
                        onClick={() => profiles.requestReset(name)}
                      >
                        <span aria-hidden="true">↺</span>
                      </button>
                    ))}
                  {!isRenaming &&
                    canDelete &&
                    (isDeleteCandidate ? (
                      <button
                        type="button"
                        className="link danger"
                        onClick={() => profiles.confirmDelete(name)}
                      >
                        Confirm delete
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="icon-action icon-pop"
                        aria-label={`Delete profile ${name}`}
                        title="Delete"
                        onClick={() => profiles.requestDelete(name)}
                      >
                        <span aria-hidden="true">✕</span>
                      </button>
                    ))}
                </span>
              </div>
            );
          })}
        </div>
        <div className="profile-add">
          <input
            type="text"
            placeholder="New profile name"
            value={profiles.newProfileName}
            onChange={(e) => profiles.setNewProfileName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") profiles.create();
            }}
          />
          <button
            className="secondary"
            onClick={profiles.create}
            disabled={!profiles.newProfileName.trim()}
          >
            Add
          </button>
        </div>
      </section>
    </>
  );
}
