export default function SectionSkeleton() {
  return (
    <div class="section-card" aria-busy="true" aria-label="Loading...">
      <div class="section-card__header">
        <span class="skeleton skeleton-line" style={{ width: '60px', height: '14px', margin: 0 }} />
        <span class="skeleton skeleton-line" style={{ width: '40%', height: '12px', margin: '0 0 0 auto' }} />
      </div>
      <div class="section-card__body">
        <span class="skeleton skeleton-line" style={{ width: '90%' }} />
        <span class="skeleton skeleton-line" style={{ width: '75%' }} />
        <span class="skeleton skeleton-line" style={{ width: '55%' }} />
      </div>
    </div>
  );
}
